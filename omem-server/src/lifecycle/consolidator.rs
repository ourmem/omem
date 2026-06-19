use std::sync::Arc;

use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::domain::category::Category;
use crate::domain::memory::Memory;
use crate::domain::relation::{MemoryRelation, RelationType};
use crate::domain::types::{MemoryState, MemoryType};
use crate::embed::EmbedService;
use crate::lifecycle::tier::TierManager;
use crate::llm::{complete_json, LlmService};
use crate::store::lancedb::LanceStore;
use crate::store::SpaceStore;
use crate::store::StoreManager;

const DEFAULT_LOOKBACK_HOURS: u64 = 24;
const DEFAULT_MIN_BATCH: usize = 3;
const DEFAULT_MAX_BATCH: usize = 50;

#[derive(Serialize, Deserialize, Debug)]
pub struct ConsolidationDecision {
    pub action: String,
    pub observation_indices: Vec<usize>,
    pub l0_abstract: String,
    pub l1_overview: String,
    pub l2_content: String,
    pub category: String,
    pub tags: Vec<String>,
    pub reason: String,
}

#[derive(Serialize, Deserialize, Debug)]
pub struct ConsolidationResult {
    pub decisions: Vec<ConsolidationDecision>,
}

pub struct ConsolidationConfig {
    pub lookback_hours: u64,
    pub min_batch: usize,
    pub max_batch: usize,
}

impl Default for ConsolidationConfig {
    fn default() -> Self {
        Self {
            lookback_hours: DEFAULT_LOOKBACK_HOURS,
            min_batch: DEFAULT_MIN_BATCH,
            max_batch: DEFAULT_MAX_BATCH,
        }
    }
}

pub struct Consolidator {
    store_manager: Arc<StoreManager>,
    space_store: Arc<SpaceStore>,
    llm: Arc<dyn LlmService>,
    embed: Arc<dyn EmbedService>,
    config: ConsolidationConfig,
}

impl Consolidator {
    pub fn new(
        store_manager: Arc<StoreManager>,
        space_store: Arc<SpaceStore>,
        llm: Arc<dyn LlmService>,
        embed: Arc<dyn EmbedService>,
        config: ConsolidationConfig,
    ) -> Self {
        Self {
            store_manager,
            space_store,
            llm,
            embed,
            config,
        }
    }

    pub async fn run_once(&self) -> Result<ConsolidationStats, crate::domain::error::OmemError> {
        let spaces = self.space_store.list_all_spaces().await?;
        let mut stats = ConsolidationStats::default();

        for space in &spaces {
            match self.consolidate_space(&space.id).await {
                Ok(space_stats) => {
                    stats.spaces_processed += 1;
                    stats.observations_reviewed += space_stats.observations_reviewed;
                    stats.promoted += space_stats.promoted;
                    stats.merged += space_stats.merged;
                    stats.skipped += space_stats.skipped;
                    stats.tiers_updated += space_stats.tiers_updated;
                }
                Err(e) => {
                    warn!(space_id = %space.id, error = %e, "consolidation failed for space");
                    stats.errors += 1;
                }
            }
        }

        info!(
            spaces = stats.spaces_processed,
            reviewed = stats.observations_reviewed,
            promoted = stats.promoted,
            merged = stats.merged,
            skipped = stats.skipped,
            tiers = stats.tiers_updated,
            errors = stats.errors,
            "consolidation cycle complete"
        );

        Ok(stats)
    }

    async fn consolidate_space(
        &self,
        space_id: &str,
    ) -> Result<ConsolidationStats, crate::domain::error::OmemError> {
        let store = self.store_manager.get_store(space_id).await?;
        let mut stats = ConsolidationStats::default();
        stats.spaces_processed = 1;

        let candidates = self.find_candidates(&store).await?;
        stats.observations_reviewed = candidates.len();

        if candidates.len() < self.config.min_batch {
            return Ok(stats);
        }

        let batch: Vec<&Memory> = candidates.iter().take(self.config.max_batch).collect();

        let (system, user) = build_consolidation_prompt(&batch);
        let result: ConsolidationResult = complete_json(self.llm.as_ref(), &system, &user).await?;

        for decision in &result.decisions {
            match decision.action.as_str() {
                "PROMOTE" | "MERGE_PROMOTE" => {
                    let old_ids: Vec<String> = decision
                        .observation_indices
                        .iter()
                        .filter_map(|&i| batch.get(i).map(|m| m.id.clone()))
                        .collect();

                    if old_ids.is_empty() {
                        warn!(reason = %decision.reason, "decision referenced no valid indices");
                        continue;
                    }

                    let source_mem = batch
                        .get(decision.observation_indices[0])
                        .expect("validated above");

                    let category: Category = decision
                        .category
                        .parse()
                        .unwrap_or(source_mem.category.clone());

                    let mut new_mem =
                        Memory::new(&decision.l2_content, category, MemoryType::Insight, &source_mem.tenant_id);
                    new_mem.l0_abstract = decision.l0_abstract.clone();
                    new_mem.l1_overview = decision.l1_overview.clone();
                    new_mem.l2_content = decision.l2_content.clone();
                    new_mem.content = decision.l0_abstract.clone();
                    new_mem.tags = decision.tags.clone();
                    new_mem.space_id = space_id.to_string();
                    new_mem.source = Some("consolidation".to_string());

                    for old_id in &old_ids {
                        new_mem.relations.push(MemoryRelation {
                            relation_type: RelationType::Supersedes,
                            target_id: old_id.clone(),
                            context_label: None,
                        });
                    }

                    let texts = [new_mem.l0_abstract.clone()];
                    let mut vectors = self.embed.embed(&texts).await?;
                    let vector = vectors.remove(0);

                    match store
                        .supersede_batch(&new_mem, Some(&vector), &old_ids)
                        .await
                    {
                        Ok(()) => {
                            if old_ids.len() > 1 {
                                stats.merged += 1;
                            } else {
                                stats.promoted += 1;
                            }
                        }
                        Err(e) => {
                            warn!(
                                new_id = %new_mem.id,
                                error = %e,
                                "supersede_batch failed during consolidation"
                            );
                            stats.errors += 1;
                        }
                    }
                }
                "SKIP" => {
                    stats.skipped += decision.observation_indices.len();
                }
                other => {
                    warn!(action = %other, "unknown consolidation action");
                }
            }
        }

        let tier_updates = self.run_tier_evaluation(&store).await?;
        stats.tiers_updated = tier_updates;

        Ok(stats)
    }

    async fn find_candidates(
        &self,
        store: &Arc<LanceStore>,
    ) -> Result<Vec<Memory>, crate::domain::error::OmemError> {
        let cutoff = chrono::Utc::now()
            - chrono::TimeDelta::try_hours(self.config.lookback_hours as i64)
                .unwrap_or_else(chrono::TimeDelta::zero);
        let cutoff_str = cutoff.to_rfc3339();

        let all = store.list_all_active().await?;
        let candidates: Vec<Memory> = all
            .into_iter()
            .filter(|m| {
                matches!(m.memory_type, MemoryType::Session)
                    && m.created_at >= cutoff_str
                    && !matches!(m.state, MemoryState::Superseded)
            })
            .collect();

        Ok(candidates)
    }

    async fn run_tier_evaluation(
        &self,
        store: &Arc<LanceStore>,
    ) -> Result<usize, crate::domain::error::OmemError> {
        let tier_manager = TierManager::with_defaults();
        let active = store.list_all_active().await?;
        let mut updated = 0;

        for mem in &active {
            let new_tier = tier_manager.evaluate_tier(mem);
            if new_tier != mem.tier {
                let mut updated_mem = mem.clone();
                updated_mem.tier = new_tier;
                updated_mem.updated_at = chrono::Utc::now().to_rfc3339();
                store.update(&updated_mem, None).await?;
                updated += 1;
            }
        }

        Ok(updated)
    }
}

#[derive(Debug, Default)]
pub struct ConsolidationStats {
    pub spaces_processed: usize,
    pub observations_reviewed: usize,
    pub promoted: usize,
    pub merged: usize,
    pub skipped: usize,
    pub tiers_updated: usize,
    pub errors: usize,
}

fn build_consolidation_prompt(observations: &[&Memory]) -> (String, String) {
    let system = CONSOLIDATION_SYSTEM_PROMPT.to_string();

    let mut user = String::with_capacity(2048);
    user.push_str("## Recent Observations\n\n");

    for (i, mem) in observations.iter().enumerate() {
        let age = format_age(&mem.created_at);
        user.push_str(&format!(
            "[{}] (category: {}, age: {}) {}\n  Detail: {}\n\n",
            i, mem.category, age, mem.l0_abstract, mem.l2_content,
        ));
    }

    user.push_str(&format!(
        "Analyze these {} observations and return consolidation decisions.\n",
        observations.len()
    ));

    (system, user)
}

fn format_age(created_at: &str) -> String {
    let Ok(created) = chrono::DateTime::parse_from_rfc3339(created_at) else {
        return "unknown".to_string();
    };
    let duration = chrono::Utc::now().signed_duration_since(created);
    let hours = duration.num_hours();
    if hours < 1 {
        return "< 1 hour ago".to_string();
    }
    if hours == 1 {
        return "1 hour ago".to_string();
    }
    if hours < 24 {
        return format!("{hours} hours ago");
    }
    let days = duration.num_days();
    if days == 1 {
        "1 day ago".to_string()
    } else {
        format!("{days} days ago")
    }
}

const CONSOLIDATION_SYSTEM_PROMPT: &str = r#"You are a memory consolidation engine. Your job is to review recent short-lived observations (session memories) and decide which contain durable, long-term facts worth preserving.

## Operations

- **PROMOTE**: A single observation contains a durable fact. Create a new long-term memory from it, superseding the original observation.
- **MERGE_PROMOTE**: Multiple observations are about the same topic and should be consolidated into a single long-term memory. List all their indices in `observation_indices`. The new memory should synthesize all of them.
- **SKIP**: The observation is ephemeral, already captured elsewhere, or not worth long-term storage. Provide the indices in `observation_indices`.

## Rules

1. Every observation index (0..N-1) must appear in exactly one decision.
2. Prefer MERGE_PROMOTE when multiple observations discuss the same entity, preference, or event — one consolidated memory is better than several near-duplicates.
3. SKIP observations that are:
   - Purely ephemeral (greetings, session artifacts, temporary state)
   - Already well-covered by another PROMOTE/MERGE_PROMOTE decision
   - Too vague to be useful as a standalone memory
4. For PROMOTE and MERGE_PROMOTE, produce rich layered content:
   - `l0_abstract`: One-sentence summary for scanning
   - `l1_overview`: 2-4 line structured markdown summary with key attributes
   - `l2_content`: Full narrative preserving all relevant detail from the source observations
5. Choose the most specific `category` that fits: profile, preferences, entities, events, cases, patterns.
6. Preserve the original language — do not translate.
7. Tag generously with relevant keywords.

## Output Format
Return ONLY valid JSON:
{"decisions": [
  {"action": "PROMOTE", "observation_indices": [0], "l0_abstract": "...", "l1_overview": "...", "l2_content": "...", "category": "...", "tags": ["..."], "reason": "durable preference"},
  {"action": "MERGE_PROMOTE", "observation_indices": [1, 3, 5], "l0_abstract": "...", "l1_overview": "...", "l2_content": "...", "category": "...", "tags": ["..."], "reason": "three observations about same project"},
  {"action": "SKIP", "observation_indices": [2, 4], "l0_abstract": "", "l1_overview": "", "l2_content": "", "category": "", "tags": [], "reason": "ephemeral session artifacts"}
]}
"#;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::category::Category;
    use crate::domain::types::MemoryType;

    fn make_session_memory(content: &str, l0: &str, hours_ago: i64) -> Memory {
        let mut mem = Memory::new(content, Category::Preferences, MemoryType::Session, "t-001");
        mem.l0_abstract = l0.to_string();
        mem.l2_content = content.to_string();
        let created = chrono::Utc::now()
            - chrono::TimeDelta::try_hours(hours_ago).unwrap_or_else(chrono::TimeDelta::zero);
        mem.created_at = created.to_rfc3339();
        mem.updated_at = mem.created_at.clone();
        mem
    }

    #[test]
    fn test_build_consolidation_prompt_includes_all_observations() {
        let m1 = make_session_memory("likes dark mode", "User prefers dark mode", 2);
        let m2 = make_session_memory("uses vim keybinds", "User uses vim keybindings", 1);
        let batch: Vec<&Memory> = vec![&m1, &m2];

        let (system, user) = build_consolidation_prompt(&batch);

        assert!(system.contains("memory consolidation engine"));
        assert!(user.contains("[0]"));
        assert!(user.contains("[1]"));
        assert!(user.contains("dark mode"));
        assert!(user.contains("vim"));
        assert!(user.contains("2 observations"));
    }

    #[test]
    fn test_format_age_hours() {
        let now = chrono::Utc::now();
        let two_hours_ago =
            now - chrono::TimeDelta::try_hours(2).unwrap_or_else(chrono::TimeDelta::zero);
        assert_eq!(format_age(&two_hours_ago.to_rfc3339()), "2 hours ago");
    }

    #[test]
    fn test_format_age_days() {
        let now = chrono::Utc::now();
        let three_days_ago =
            now - chrono::TimeDelta::try_days(3).unwrap_or_else(chrono::TimeDelta::zero);
        assert_eq!(format_age(&three_days_ago.to_rfc3339()), "3 days ago");
    }

    #[test]
    fn test_consolidation_decision_deserialize() {
        let json = r#"{
            "decisions": [
                {
                    "action": "PROMOTE",
                    "observation_indices": [0],
                    "l0_abstract": "User prefers dark mode",
                    "l1_overview": "**Preference**: Dark mode\n**Context**: IDE and terminal",
                    "l2_content": "The user expressed a preference for dark mode across their IDE and terminal.",
                    "category": "preferences",
                    "tags": ["ui", "dark-mode"],
                    "reason": "durable preference"
                },
                {
                    "action": "SKIP",
                    "observation_indices": [1, 2],
                    "l0_abstract": "",
                    "l1_overview": "",
                    "l2_content": "",
                    "category": "",
                    "tags": [],
                    "reason": "ephemeral"
                }
            ]
        }"#;

        let result: ConsolidationResult = serde_json::from_str(json).expect("should parse");
        assert_eq!(result.decisions.len(), 2);
        assert_eq!(result.decisions[0].action, "PROMOTE");
        assert_eq!(result.decisions[0].observation_indices, vec![0]);
        assert_eq!(result.decisions[1].action, "SKIP");
        assert_eq!(result.decisions[1].observation_indices, vec![1, 2]);
    }

    #[test]
    fn test_consolidation_config_defaults() {
        let config = ConsolidationConfig::default();
        assert_eq!(config.lookback_hours, 24);
        assert_eq!(config.min_batch, 3);
        assert_eq!(config.max_batch, 50);
    }
}
