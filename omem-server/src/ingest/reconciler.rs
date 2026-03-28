use std::collections::HashMap;
use std::sync::Arc;

use tracing::{info, warn};

use crate::domain::category::Category;
use crate::domain::error::OmemError;
use crate::domain::memory::Memory;
use crate::domain::relation::{MemoryRelation, RelationType};
use crate::domain::types::MemoryType;
use crate::embed::EmbedService;
use crate::ingest::preference_slots;
use crate::ingest::prompts;
use crate::ingest::types::{BatchDedupResult, ExtractedFact, ReconcileResult};
use crate::llm::{complete_json, LlmService};
use crate::store::LanceStore;

const DEFAULT_MAX_EXISTING: usize = 60;
const DEFAULT_MAX_PER_FACT: usize = 5;
const DEFAULT_MIN_SIMILARITY: f32 = 0.3;

pub struct Reconciler {
    llm: Arc<dyn LlmService>,
    store: Arc<LanceStore>,
    embed: Arc<dyn EmbedService>,
    max_existing: usize,
    max_per_fact: usize,
    min_similarity: f32,
}

impl Reconciler {
    pub fn new(
        llm: Arc<dyn LlmService>,
        store: Arc<LanceStore>,
        embed: Arc<dyn EmbedService>,
    ) -> Self {
        Self {
            llm,
            store,
            embed,
            max_existing: DEFAULT_MAX_EXISTING,
            max_per_fact: DEFAULT_MAX_PER_FACT,
            min_similarity: DEFAULT_MIN_SIMILARITY,
        }
    }

    pub async fn reconcile(
        &self,
        facts: &[ExtractedFact],
        tenant_id: &str,
    ) -> Result<Vec<Memory>, OmemError> {
        if facts.is_empty() {
            return Ok(Vec::new());
        }

        let (existing, all_searches_failed) = self.gather_existing(facts).await;

        if existing.is_empty() && all_searches_failed {
            return Err(OmemError::Internal(
                "all searches failed during reconciliation — refusing to create duplicates"
                    .to_string(),
            ));
        }

        if existing.is_empty() {
            if facts.len() > 1 {
                let deduped = self.batch_self_dedup(facts).await?;
                return self.create_all_facts(&deduped, tenant_id).await;
            }
            return self.create_all_facts(facts, tenant_id).await;
        }

        let mut created_memories = Vec::new();
        let mut remaining_facts: Vec<(usize, &ExtractedFact)> = Vec::new();

        for (idx, fact) in facts.iter().enumerate() {
            if self.preference_slot_guard(fact, &existing).await? {
                let mem = self.create_fact_memory(fact, tenant_id).await?;
                created_memories.push(mem);
            } else {
                remaining_facts.push((idx, fact));
            }
        }

        if remaining_facts.is_empty() {
            return Ok(created_memories);
        }

        let remaining_extracted: Vec<ExtractedFact> =
            remaining_facts.iter().map(|(_, f)| (*f).clone()).collect();

        let (id_map, int_to_uuid) = build_id_maps(&existing);

        let (system, user) =
            prompts::build_reconcile_prompt(&remaining_extracted, &existing, &id_map);
        let result: ReconcileResult = complete_json(self.llm.as_ref(), &system, &user).await?;

        for decision in &result.decisions {
            let action = decision.action.to_uppercase();
            let (_, fact) = remaining_facts.get(decision.fact_index).ok_or_else(|| {
                OmemError::Llm(format!("invalid fact_index: {}", decision.fact_index))
            })?;

            match action.as_str() {
                "CREATE" => {
                    let mem = self.create_fact_memory(fact, tenant_id).await?;
                    created_memories.push(mem);
                }
                "MERGE" => {
                    let match_idx = decision.match_index.ok_or_else(|| {
                        OmemError::Llm("MERGE decision missing match_index".to_string())
                    })?;
                    let real_id = int_to_uuid.get(&match_idx).ok_or_else(|| {
                        OmemError::Llm(format!("invalid match_index: {match_idx}"))
                    })?;

                    let target = self
                        .store
                        .get_by_id(real_id)
                        .await?
                        .ok_or_else(|| OmemError::NotFound(format!("memory {real_id}")))?;

                    if target.memory_type.is_pinned() {
                        warn!(
                            memory_id = %real_id,
                            "MERGE attempted on pinned memory — downgrading to CREATE"
                        );
                        let mem = self.create_fact_memory(fact, tenant_id).await?;
                        created_memories.push(mem);
                        continue;
                    }

                    let merged_content = decision
                        .merged_content
                        .as_deref()
                        .unwrap_or(&fact.l0_abstract);

                    let mut updated = target;
                    updated.content = merged_content.to_string();
                    updated.l0_abstract = merged_content.to_string();
                    updated.updated_at = chrono::Utc::now().to_rfc3339();

                    let embeddings = self
                        .embed
                        .embed(&[merged_content.to_string()])
                        .await?;
                    let vector = embeddings.first().map(|v| v.as_slice());

                    self.store.update(&updated, vector).await?;
                    created_memories.push(updated);
                }
                "SKIP" => {}
                "SUPERSEDE" => {
                    self.handle_supersede(fact, &decision.match_index, &int_to_uuid, tenant_id, &mut created_memories).await?;
                }
                "SUPPORT" => {
                    self.handle_support(fact, &decision.match_index, &decision.context_label, &int_to_uuid, &mut created_memories).await?;
                }
                "CONTEXTUALIZE" => {
                    self.handle_contextualize(fact, &decision.match_index, &decision.context_label, &int_to_uuid, tenant_id, &mut created_memories).await?;
                }
                "CONTRADICT" => {
                    self.handle_contradict(fact, &decision.match_index, &int_to_uuid, tenant_id, &mut created_memories).await?;
                }
                other => {
                    warn!(action = %other, "unknown reconciliation action — treating as CREATE");
                    let mem = self.create_fact_memory(fact, tenant_id).await?;
                    created_memories.push(mem);
                }
            }
        }

        Ok(created_memories)
    }

    async fn preference_slot_guard(
        &self,
        fact: &ExtractedFact,
        existing: &[Memory],
    ) -> Result<bool, OmemError> {
        let category: Category = fact.category.parse().unwrap_or(Category::Profile);
        if category != Category::Preferences {
            return Ok(false);
        }

        let candidate_slot = match preference_slots::infer_preference_slot(&fact.l0_abstract) {
            Some(s) => s,
            None => return Ok(false),
        };

        for mem in existing {
            if mem.category != Category::Preferences {
                continue;
            }
            if let Some(existing_slot) =
                preference_slots::infer_preference_slot(&mem.l0_abstract)
            {
                if preference_slots::is_same_brand_different_item(&candidate_slot, &existing_slot) {
                    return Ok(true);
                }
            }
        }

        Ok(false)
    }

    async fn handle_supersede(
        &self,
        fact: &ExtractedFact,
        match_index: &Option<usize>,
        int_to_uuid: &HashMap<usize, String>,
        tenant_id: &str,
        created_memories: &mut Vec<Memory>,
    ) -> Result<(), OmemError> {
        let match_idx = match_index.ok_or_else(|| {
            OmemError::Llm("SUPERSEDE decision missing match_index".to_string())
        })?;
        let real_id = int_to_uuid.get(&match_idx).ok_or_else(|| {
            OmemError::Llm(format!("invalid match_index: {match_idx}"))
        })?;

        let old = self
            .store
            .get_by_id(real_id)
            .await?
            .ok_or_else(|| OmemError::NotFound(format!("memory {real_id}")))?;

        if old.memory_type.is_pinned() {
            warn!(
                memory_id = %real_id,
                "SUPERSEDE attempted on pinned memory — downgrading to CREATE"
            );
            let mem = self.create_fact_memory(fact, tenant_id).await?;
            created_memories.push(mem);
            return Ok(());
        }

        let new_mem = self.create_fact_memory(fact, tenant_id).await?;

        let mut archived = old;
        archived.invalidated_at = Some(chrono::Utc::now().to_rfc3339());
        archived.superseded_by = Some(new_mem.id.clone());
        archived.updated_at = chrono::Utc::now().to_rfc3339();
        self.store.update(&archived, None).await?;

        created_memories.push(new_mem);
        Ok(())
    }

    async fn handle_support(
        &self,
        _fact: &ExtractedFact,
        match_index: &Option<usize>,
        context_label: &Option<String>,
        int_to_uuid: &HashMap<usize, String>,
        created_memories: &mut Vec<Memory>,
    ) -> Result<(), OmemError> {
        let match_idx = match_index.ok_or_else(|| {
            OmemError::Llm("SUPPORT decision missing match_index".to_string())
        })?;
        let real_id = int_to_uuid.get(&match_idx).ok_or_else(|| {
            OmemError::Llm(format!("invalid match_index: {match_idx}"))
        })?;

        let mut target = self
            .store
            .get_by_id(real_id)
            .await?
            .ok_or_else(|| OmemError::NotFound(format!("memory {real_id}")))?;

        target.confidence = (target.confidence + 0.1).min(1.0);
        target.relations.push(MemoryRelation {
            relation_type: RelationType::Supports,
            target_id: real_id.clone(),
            context_label: context_label.clone(),
        });
        target.updated_at = chrono::Utc::now().to_rfc3339();

        self.store.update(&target, None).await?;
        created_memories.push(target);
        Ok(())
    }

    async fn handle_contextualize(
        &self,
        fact: &ExtractedFact,
        match_index: &Option<usize>,
        context_label: &Option<String>,
        int_to_uuid: &HashMap<usize, String>,
        tenant_id: &str,
        created_memories: &mut Vec<Memory>,
    ) -> Result<(), OmemError> {
        let match_idx = match_index.ok_or_else(|| {
            OmemError::Llm("CONTEXTUALIZE decision missing match_index".to_string())
        })?;
        let real_id = int_to_uuid.get(&match_idx).ok_or_else(|| {
            OmemError::Llm(format!("invalid match_index: {match_idx}"))
        })?;

        let mut new_mem = self.create_fact_memory(fact, tenant_id).await?;
        new_mem.relations.push(MemoryRelation {
            relation_type: RelationType::Contextualizes,
            target_id: real_id.clone(),
            context_label: context_label.clone(),
        });

        let ctx_source = fact.source_text.as_deref().unwrap_or(&fact.l0_abstract);
        let embeddings = self.embed.embed(&[ctx_source.to_string()]).await?;
        let vector = embeddings.first().map(|v| v.as_slice());
        self.store.update(&new_mem, vector).await?;

        created_memories.push(new_mem);
        Ok(())
    }

    async fn handle_contradict(
        &self,
        fact: &ExtractedFact,
        match_index: &Option<usize>,
        int_to_uuid: &HashMap<usize, String>,
        tenant_id: &str,
        created_memories: &mut Vec<Memory>,
    ) -> Result<(), OmemError> {
        let match_idx = match_index.ok_or_else(|| {
            OmemError::Llm("CONTRADICT decision missing match_index".to_string())
        })?;
        let real_id = int_to_uuid.get(&match_idx).ok_or_else(|| {
            OmemError::Llm(format!("invalid match_index: {match_idx}"))
        })?;

        let old = self
            .store
            .get_by_id(real_id)
            .await?
            .ok_or_else(|| OmemError::NotFound(format!("memory {real_id}")))?;

        let category: Category = fact.category.parse().unwrap_or(Category::Profile);
        if category.is_temporal_versioned() {
            return self
                .handle_supersede(fact, match_index, int_to_uuid, tenant_id, created_memories)
                .await;
        }

        let mut new_mem = self.create_fact_memory(fact, tenant_id).await?;
        new_mem.relations.push(MemoryRelation {
            relation_type: RelationType::Contradicts,
            target_id: real_id.clone(),
            context_label: None,
        });

        let contra_source = fact.source_text.as_deref().unwrap_or(&fact.l0_abstract);
        let embeddings = self.embed.embed(&[contra_source.to_string()]).await?;
        let vector = embeddings.first().map(|v| v.as_slice());
        self.store.update(&new_mem, vector).await?;

        let mut old_updated = old;
        old_updated.relations.push(MemoryRelation {
            relation_type: RelationType::Contradicts,
            target_id: new_mem.id.clone(),
            context_label: None,
        });
        old_updated.updated_at = chrono::Utc::now().to_rfc3339();
        self.store.update(&old_updated, None).await?;

        created_memories.push(new_mem);
        Ok(())
    }

    async fn batch_self_dedup(
        &self,
        facts: &[ExtractedFact],
    ) -> Result<Vec<ExtractedFact>, OmemError> {
        let (system, user) = prompts::build_batch_dedup_prompt(facts);

        let result: BatchDedupResult = complete_json(self.llm.as_ref(), &system, &user).await?;

        let deduped: Vec<ExtractedFact> = result
            .keep_indices
            .iter()
            .filter_map(|&idx| facts.get(idx).cloned())
            .collect();

        if deduped.is_empty() {
            // Safety: if LLM returns garbage, keep all facts
            Ok(facts.to_vec())
        } else {
            info!(
                original = facts.len(),
                deduped = deduped.len(),
                removed = facts.len() - deduped.len(),
                "batch self-dedup completed"
            );
            Ok(deduped)
        }
    }

    async fn gather_existing(
        &self,
        facts: &[ExtractedFact],
    ) -> (Vec<Memory>, bool) {
        let mut seen_ids: HashMap<String, Memory> = HashMap::new();
        let mut any_search_succeeded = false;
        let mut total_count = 0;

        for fact in facts {
            if total_count >= self.max_existing {
                break;
            }

            let search_text = fact.source_text.as_deref().unwrap_or(&fact.l0_abstract);

            let embed_result = self
                .embed
                .embed(std::slice::from_ref(&search_text.to_string()))
                .await;

            if let Ok(vectors) = embed_result {
                if let Some(query_vec) = vectors.first() {
                    match self
                        .store
                        .vector_search(query_vec, self.max_per_fact, self.min_similarity, None, None)
                        .await
                    {
                        Ok(results) => {
                            any_search_succeeded = true;
                            for (mem, _score) in results {
                                if total_count >= self.max_existing {
                                    break;
                                }
                                if !seen_ids.contains_key(&mem.id) {
                                    seen_ids.insert(mem.id.clone(), mem);
                                    total_count += 1;
                                }
                            }
                        }
                        Err(e) => {
                            warn!(error = %e, "vector search failed during gather");
                        }
                    }
                }
            } else {
                warn!("embedding failed during gather");
            }

            let fts_query = fact.source_text.as_deref()
                .map(|s| s.chars().take(200).collect::<String>())
                .unwrap_or_else(|| fact.l0_abstract.clone());

            match self
                .store
                .fts_search(&fts_query, self.max_per_fact, None, None)
                .await
            {
                Ok(results) => {
                    any_search_succeeded = true;
                    for (mem, _score) in results {
                        if total_count >= self.max_existing {
                            break;
                        }
                        if !seen_ids.contains_key(&mem.id) {
                            seen_ids.insert(mem.id.clone(), mem);
                            total_count += 1;
                        }
                    }
                }
                Err(e) => {
                    warn!(error = %e, "FTS search failed during gather");
                }
            }
        }

        let all_failed = !any_search_succeeded;
        (seen_ids.into_values().collect(), all_failed)
    }

    async fn create_all_facts(
        &self,
        facts: &[ExtractedFact],
        tenant_id: &str,
    ) -> Result<Vec<Memory>, OmemError> {
        let mut memories = Vec::with_capacity(facts.len());
        for fact in facts {
            let mem = self.create_fact_memory(fact, tenant_id).await?;
            memories.push(mem);
        }
        Ok(memories)
    }

    async fn create_fact_memory(
        &self,
        fact: &ExtractedFact,
        tenant_id: &str,
    ) -> Result<Memory, OmemError> {
        let category: Category = fact
            .category
            .parse()
            .unwrap_or(Category::Profile);

        let source = fact.source_text.as_deref().unwrap_or(&fact.l0_abstract);

        let mut mem = Memory::new(source, category, MemoryType::Insight, tenant_id);
        mem.l0_abstract = fact.l0_abstract.clone();
        mem.l1_overview = fact.l1_overview.clone();
        mem.l2_content = fact.l2_content.clone();
        mem.tags = fact.tags.clone();

        let embeddings = self.embed.embed(std::slice::from_ref(&source.to_string())).await?;
        let vector = embeddings.first().map(|v| v.as_slice());

        self.store.create(&mem, vector).await?;
        Ok(mem)
    }
}

fn build_id_maps(existing: &[Memory]) -> (Vec<(usize, &str)>, HashMap<usize, String>) {
    let id_map: Vec<(usize, &str)> = existing
        .iter()
        .enumerate()
        .map(|(i, m)| (i, m.id.as_str()))
        .collect();

    let int_to_uuid: HashMap<usize, String> = id_map
        .iter()
        .map(|(i, uuid)| (*i, uuid.to_string()))
        .collect();

    (id_map, int_to_uuid)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest::types::ExtractedFact;
    use std::sync::Mutex;
    use tempfile::TempDir;

    struct MockLlm {
        response: Mutex<String>,
    }

    impl MockLlm {
        fn new(json_response: &str) -> Self {
            Self {
                response: Mutex::new(json_response.to_string()),
            }
        }
    }

    #[async_trait::async_trait]
    impl LlmService for MockLlm {
        async fn complete_text(&self, _system: &str, _user: &str) -> Result<String, OmemError> {
            Ok(self.response.lock().expect("lock").clone())
        }
    }

    struct MockEmbed;

    #[async_trait::async_trait]
    impl EmbedService for MockEmbed {
        async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, OmemError> {
            Ok(texts.iter().map(|_| vec![0.0; 1024]).collect())
        }
        fn dimensions(&self) -> usize {
            1024
        }
    }

    struct CapturingLlm {
        response: Mutex<String>,
        captured_user: Mutex<Option<String>>,
    }

    impl CapturingLlm {
        fn new(json_response: &str) -> Self {
            Self {
                response: Mutex::new(json_response.to_string()),
                captured_user: Mutex::new(None),
            }
        }

        fn captured_user(&self) -> Option<String> {
            self.captured_user.lock().expect("lock").clone()
        }
    }

    #[async_trait::async_trait]
    impl LlmService for CapturingLlm {
        async fn complete_text(&self, _system: &str, user: &str) -> Result<String, OmemError> {
            *self.captured_user.lock().expect("lock") = Some(user.to_string());
            Ok(self.response.lock().expect("lock").clone())
        }
    }

    async fn setup() -> (Arc<LanceStore>, TempDir) {
        let dir = TempDir::new().expect("temp dir");
        let store = LanceStore::new(dir.path().to_str().expect("path"))
            .await
            .expect("store");
        store.init_table().await.expect("init");
        (Arc::new(store), dir)
    }

    fn make_fact(abstract_text: &str, category: &str) -> ExtractedFact {
        ExtractedFact {
            l0_abstract: abstract_text.to_string(),
            l1_overview: format!("Overview: {abstract_text}"),
            l2_content: format!("Detail: {abstract_text}"),
            category: category.to_string(),
            tags: vec![],
            source_text: None,
        }
    }

    #[tokio::test]
    async fn test_reconcile_empty_store() {
        let (store, _dir) = setup().await;
        let llm = Arc::new(MockLlm::new(r#"{"keep_indices": [0, 1]}"#));
        let embed = Arc::new(MockEmbed);

        let reconciler = Reconciler::new(llm, store.clone(), embed);

        let facts = vec![
            make_fact("User prefers Rust", "preferences"),
            make_fact("User works at Stripe", "profile"),
        ];

        let result = reconciler.reconcile(&facts, "t-001").await.expect("reconcile");

        assert_eq!(result.len(), 2);
        assert_eq!(result[0].l0_abstract, "User prefers Rust");
        assert_eq!(result[0].tenant_id, "t-001");
        assert_eq!(result[0].memory_type, MemoryType::Insight);
        assert_eq!(result[1].l0_abstract, "User works at Stripe");
    }

    #[tokio::test]
    async fn test_reconcile_skip_duplicate() {
        let (store, _dir) = setup().await;
        let embed = Arc::new(MockEmbed);

        let existing = Memory::new(
            "User prefers Rust",
            Category::Preferences,
            MemoryType::Insight,
            "t-001",
        );
        store.create(&existing, Some(&vec![0.0; 1024])).await.expect("create");

        let skip_response = r#"{"decisions":[{"action":"SKIP","fact_index":0,"match_index":0,"reason":"duplicate"}]}"#;
        let llm = Arc::new(MockLlm::new(skip_response));

        let reconciler = Reconciler::new(llm, store.clone(), embed);
        let facts = vec![make_fact("User prefers Rust", "preferences")];

        let result = reconciler.reconcile(&facts, "t-001").await.expect("reconcile");
        assert!(result.is_empty());
    }

    #[tokio::test]
    async fn test_reconcile_merge() {
        let (store, _dir) = setup().await;
        let embed = Arc::new(MockEmbed);

        let mut existing = Memory::new(
            "User prefers Rust",
            Category::Preferences,
            MemoryType::Insight,
            "t-001",
        );
        existing.l0_abstract = "User prefers Rust".to_string();
        store.create(&existing, Some(&vec![0.0; 1024])).await.expect("create");

        let merge_response = r#"{"decisions":[{"action":"MERGE","fact_index":0,"match_index":0,"merged_content":"User prefers Rust for its safety and performance","reason":"adds detail"}]}"#;
        let llm = Arc::new(MockLlm::new(merge_response));

        let reconciler = Reconciler::new(llm, store.clone(), embed);
        let facts = vec![make_fact("User likes Rust for safety and performance", "preferences")];

        let result = reconciler.reconcile(&facts, "t-001").await.expect("reconcile");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].content, "User prefers Rust for its safety and performance");

        let updated = store.get_by_id(&existing.id).await.expect("get").expect("found");
        assert_eq!(updated.content, "User prefers Rust for its safety and performance");
    }

    #[tokio::test]
    async fn test_reconcile_supersede() {
        let (store, _dir) = setup().await;
        let embed = Arc::new(MockEmbed);

        let mut existing = Memory::new(
            "User works at Google",
            Category::Profile,
            MemoryType::Insight,
            "t-001",
        );
        existing.l0_abstract = "User works at Google".to_string();
        store.create(&existing, Some(&vec![0.0; 1024])).await.expect("create");

        let supersede_response = r#"{"decisions":[{"action":"SUPERSEDE","fact_index":0,"match_index":0,"reason":"user changed jobs"}]}"#;
        let llm = Arc::new(MockLlm::new(supersede_response));

        let reconciler = Reconciler::new(llm, store.clone(), embed);
        let facts = vec![make_fact("User now works at Stripe", "profile")];

        let result = reconciler.reconcile(&facts, "t-001").await.expect("reconcile");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].l0_abstract, "User now works at Stripe");

        let old = store.get_by_id(&existing.id).await.expect("get").expect("found");
        assert!(old.invalidated_at.is_some());
        assert_eq!(old.superseded_by.as_deref(), Some(result[0].id.as_str()));
    }

    #[tokio::test]
    async fn test_pinned_protection() {
        let (store, _dir) = setup().await;
        let embed = Arc::new(MockEmbed);

        let mut pinned = Memory::new(
            "Important: always use HTTPS",
            Category::Preferences,
            MemoryType::Pinned,
            "t-001",
        );
        pinned.l0_abstract = "Important: always use HTTPS".to_string();
        store.create(&pinned, Some(&vec![0.0; 1024])).await.expect("create");

        let merge_response = r#"{"decisions":[{"action":"MERGE","fact_index":0,"match_index":0,"merged_content":"merged text","reason":"refine"}]}"#;
        let llm = Arc::new(MockLlm::new(merge_response));

        let reconciler = Reconciler::new(llm, store.clone(), embed);
        let facts = vec![make_fact("Use HTTPS everywhere", "preferences")];

        let result = reconciler.reconcile(&facts, "t-001").await.expect("reconcile");

        assert_eq!(result.len(), 1);
        assert_ne!(result[0].id, pinned.id);
        assert_eq!(result[0].memory_type, MemoryType::Insight);

        let original = store.get_by_id(&pinned.id).await.expect("get").expect("found");
        assert_eq!(original.content, "Important: always use HTTPS");
        assert_eq!(original.memory_type, MemoryType::Pinned);
    }

    #[tokio::test]
    async fn test_uuid_to_int_mapping() {
        let (store, _dir) = setup().await;

        let mut m1 = Memory::new("Fact A", Category::Profile, MemoryType::Insight, "t-001");
        m1.l0_abstract = "Fact A".to_string();
        let mut m2 = Memory::new("Fact B", Category::Preferences, MemoryType::Insight, "t-001");
        m2.l0_abstract = "Fact B".to_string();

        store.create(&m1, Some(&vec![0.0; 1024])).await.expect("create");
        store.create(&m2, Some(&vec![0.0; 1024])).await.expect("create");

        let skip_response = r#"{"decisions":[{"action":"SKIP","fact_index":0,"match_index":0,"reason":"dup"}]}"#;
        let llm = Arc::new(CapturingLlm::new(skip_response));
        let embed = Arc::new(MockEmbed);

        let reconciler = Reconciler::new(llm.clone(), store.clone(), embed);
        let facts = vec![make_fact("Fact A", "profile")];

        let _ = reconciler.reconcile(&facts, "t-001").await.expect("reconcile");

        let captured = llm.captured_user().expect("captured");
        assert!(!captured.contains(&m1.id), "prompt should not contain raw UUID");
        assert!(!captured.contains(&m2.id), "prompt should not contain raw UUID");
        assert!(captured.contains("[0]"), "prompt should contain integer ID [0]");
    }

    #[tokio::test]
    async fn test_support_decision() {
        let (store, _dir) = setup().await;
        let embed = Arc::new(MockEmbed);

        let mut existing = Memory::new(
            "User likes coffee",
            Category::Preferences,
            MemoryType::Insight,
            "t-001",
        );
        existing.l0_abstract = "User likes coffee".to_string();
        existing.confidence = 0.5;
        store
            .create(&existing, Some(&vec![0.0; 1024]))
            .await
            .expect("create");

        let support_response = r#"{"decisions":[{"action":"SUPPORT","fact_index":0,"match_index":0,"context_label":"work","reason":"reinforces coffee preference"}]}"#;
        let llm = Arc::new(MockLlm::new(support_response));

        let reconciler = Reconciler::new(llm, store.clone(), embed);
        let facts = vec![make_fact("User drinks coffee at the office daily", "preferences")];

        let result = reconciler
            .reconcile(&facts, "t-001")
            .await
            .expect("reconcile");
        assert_eq!(result.len(), 1);

        let updated = store
            .get_by_id(&existing.id)
            .await
            .expect("get")
            .expect("found");
        assert!((updated.confidence - 0.6).abs() < f32::EPSILON);
        assert_eq!(updated.relations.len(), 1);
        assert_eq!(updated.relations[0].relation_type, RelationType::Supports);
        assert_eq!(
            updated.relations[0].context_label.as_deref(),
            Some("work")
        );
    }

    #[tokio::test]
    async fn test_contextualize_decision() {
        let (store, _dir) = setup().await;
        let embed = Arc::new(MockEmbed);

        let mut existing = Memory::new(
            "User likes coffee",
            Category::Preferences,
            MemoryType::Insight,
            "t-001",
        );
        existing.l0_abstract = "User likes coffee".to_string();
        store
            .create(&existing, Some(&vec![0.0; 1024]))
            .await
            .expect("create");

        let ctx_response = r#"{"decisions":[{"action":"CONTEXTUALIZE","fact_index":0,"match_index":0,"context_label":"evening","reason":"adds situational nuance"}]}"#;
        let llm = Arc::new(MockLlm::new(ctx_response));

        let reconciler = Reconciler::new(llm, store.clone(), embed);
        let facts = vec![make_fact("User prefers tea in the evening", "preferences")];

        let result = reconciler
            .reconcile(&facts, "t-001")
            .await
            .expect("reconcile");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].l0_abstract, "User prefers tea in the evening");
        assert_eq!(result[0].relations.len(), 1);
        assert_eq!(
            result[0].relations[0].relation_type,
            RelationType::Contextualizes
        );
        assert_eq!(result[0].relations[0].target_id, existing.id);
        assert_eq!(
            result[0].relations[0].context_label.as_deref(),
            Some("evening")
        );
    }

    #[tokio::test]
    async fn test_contradict_temporal_routes_to_supersede() {
        let (store, _dir) = setup().await;
        let embed = Arc::new(MockEmbed);

        let mut existing = Memory::new(
            "User prefers Python",
            Category::Preferences,
            MemoryType::Insight,
            "t-001",
        );
        existing.l0_abstract = "User prefers Python".to_string();
        store
            .create(&existing, Some(&vec![0.0; 1024]))
            .await
            .expect("create");

        let contradict_response = r#"{"decisions":[{"action":"CONTRADICT","fact_index":0,"match_index":0,"reason":"now prefers Rust"}]}"#;
        let llm = Arc::new(MockLlm::new(contradict_response));

        let reconciler = Reconciler::new(llm, store.clone(), embed);
        let facts = vec![make_fact("User now prefers Rust over Python", "preferences")];

        let result = reconciler
            .reconcile(&facts, "t-001")
            .await
            .expect("reconcile");
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].l0_abstract,
            "User now prefers Rust over Python"
        );

        let old = store
            .get_by_id(&existing.id)
            .await
            .expect("get")
            .expect("found");
        assert!(old.invalidated_at.is_some());
        assert_eq!(old.superseded_by.as_deref(), Some(result[0].id.as_str()));
    }

    #[tokio::test]
    async fn test_contradict_general_creates_with_evidence() {
        let (store, _dir) = setup().await;
        let embed = Arc::new(MockEmbed);

        let mut existing = Memory::new(
            "Deployment succeeded without issues",
            Category::Patterns,
            MemoryType::Insight,
            "t-001",
        );
        existing.l0_abstract = "Deployment succeeded without issues".to_string();
        store
            .create(&existing, Some(&vec![0.0; 1024]))
            .await
            .expect("create");

        let contradict_response = r#"{"decisions":[{"action":"CONTRADICT","fact_index":0,"match_index":0,"reason":"deployment actually had failures"}]}"#;
        let llm = Arc::new(MockLlm::new(contradict_response));

        let reconciler = Reconciler::new(llm, store.clone(), embed);
        let facts = vec![make_fact(
            "Deployment had critical failures",
            "patterns",
        )];

        let result = reconciler
            .reconcile(&facts, "t-001")
            .await
            .expect("reconcile");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].l0_abstract, "Deployment had critical failures");
        assert_eq!(result[0].relations.len(), 1);
        assert_eq!(
            result[0].relations[0].relation_type,
            RelationType::Contradicts
        );
        assert_eq!(result[0].relations[0].target_id, existing.id);

        let old = store
            .get_by_id(&existing.id)
            .await
            .expect("get")
            .expect("found");
        assert!(old.invalidated_at.is_none());
        assert_eq!(old.relations.len(), 1);
        assert_eq!(old.relations[0].relation_type, RelationType::Contradicts);
        assert_eq!(old.relations[0].target_id, result[0].id);
    }

    #[tokio::test]
    async fn test_preference_slot_guard() {
        let (store, _dir) = setup().await;
        let embed = Arc::new(MockEmbed);

        let mut existing = Memory::new(
            "喜欢星巴克的拿铁",
            Category::Preferences,
            MemoryType::Insight,
            "t-001",
        );
        existing.l0_abstract = "喜欢星巴克的拿铁".to_string();
        store
            .create(&existing, Some(&vec![0.0; 1024]))
            .await
            .expect("create");

        let llm = Arc::new(MockLlm::new("should not be called"));

        let reconciler = Reconciler::new(llm, store.clone(), embed);
        let facts = vec![make_fact("喜欢星巴克的美式", "preferences")];

        let result = reconciler
            .reconcile(&facts, "t-001")
            .await
            .expect("reconcile");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].l0_abstract, "喜欢星巴克的美式");
        assert_ne!(result[0].id, existing.id);
    }

    #[tokio::test]
    async fn test_category_aware_profile_merge() {
        let (store, _dir) = setup().await;
        let embed = Arc::new(MockEmbed);

        let mut existing = Memory::new(
            "User is a backend engineer",
            Category::Profile,
            MemoryType::Insight,
            "t-001",
        );
        existing.l0_abstract = "User is a backend engineer".to_string();
        store
            .create(&existing, Some(&vec![0.0; 1024]))
            .await
            .expect("create");

        let merge_response = r#"{"decisions":[{"action":"MERGE","fact_index":0,"match_index":0,"merged_content":"User is a senior backend engineer at Stripe","reason":"profile always merges"}]}"#;
        let llm = Arc::new(MockLlm::new(merge_response));

        let reconciler = Reconciler::new(llm, store.clone(), embed);
        let facts = vec![make_fact("User is now a senior engineer at Stripe", "profile")];

        let result = reconciler
            .reconcile(&facts, "t-001")
            .await
            .expect("reconcile");
        assert_eq!(result.len(), 1);
        assert_eq!(
            result[0].content,
            "User is a senior backend engineer at Stripe"
        );

        let updated = store
            .get_by_id(&existing.id)
            .await
            .expect("get")
            .expect("found");
        assert_eq!(
            updated.content,
            "User is a senior backend engineer at Stripe"
        );
    }

    #[tokio::test]
    async fn test_category_aware_events_append() {
        let (store, _dir) = setup().await;
        let embed = Arc::new(MockEmbed);

        let mut existing = Memory::new(
            "Deployed v2.0 to production on Jan 1",
            Category::Events,
            MemoryType::Insight,
            "t-001",
        );
        existing.l0_abstract = "Deployed v2.0 to production on Jan 1".to_string();
        store
            .create(&existing, Some(&vec![0.0; 1024]))
            .await
            .expect("create");

        let create_response = r#"{"decisions":[{"action":"CREATE","fact_index":0,"reason":"events are append-only"}]}"#;
        let llm = Arc::new(MockLlm::new(create_response));

        let reconciler = Reconciler::new(llm, store.clone(), embed);
        let facts = vec![make_fact("Deployed v2.1 hotfix on Jan 5", "events")];

        let result = reconciler
            .reconcile(&facts, "t-001")
            .await
            .expect("reconcile");
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].l0_abstract, "Deployed v2.1 hotfix on Jan 5");
        assert_ne!(result[0].id, existing.id);

        let old = store
            .get_by_id(&existing.id)
            .await
            .expect("get")
            .expect("found");
        assert!(old.invalidated_at.is_none());
        assert!(old.superseded_by.is_none());
    }
}
