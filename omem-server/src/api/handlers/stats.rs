use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use axum::extract::{Extension, Query, State};
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::server::{AppState, personal_space_id};
use crate::domain::error::OmemError;
use crate::domain::memory::Memory;
use crate::domain::space::SharingAction;
use crate::domain::tenant::AuthInfo;
use crate::domain::types::Tier;
use crate::lifecycle::decay::{DecayConfig, DecayEngine};

// ── Query params ─────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct StatsQuery {
    #[serde(default = "default_days")]
    pub days: u32,
    pub space: Option<String>,
}

fn default_days() -> u32 {
    30
}

// ── Response types ───────────────────────────────────────────────────

#[derive(Serialize)]
pub struct StatsResponse {
    pub total: usize,
    pub by_type: HashMap<String, usize>,
    pub by_category: HashMap<String, usize>,
    pub by_tier: HashMap<String, usize>,
    pub by_state: HashMap<String, usize>,
    pub by_space: HashMap<String, usize>,
    pub by_visibility: HashMap<String, usize>,
    pub by_agent: HashMap<String, usize>,
    pub timeline: Vec<TimelineEntry>,
    pub avg_importance: f32,
    pub avg_confidence: f32,
    pub total_access_count: u32,
}

#[derive(Serialize)]
pub struct TimelineEntry {
    pub date: String,
    pub count: usize,
    pub by_type: HashMap<String, usize>,
}

// ── Helper: collect memories across spaces ───────────────────────────

async fn collect_memories(
    state: &AppState,
    auth: &AuthInfo,
    space_filter: &Option<String>,
) -> Result<Vec<Memory>, OmemError> {
    match space_filter.as_deref() {
        Some(space_id) if space_id != "all" => {
            let store = state.store_manager.get_store(space_id).await?;
            store.list_all_active().await
        }
        _ => {
            let spaces = state
                .space_store
                .list_spaces_for_user(&auth.tenant_id)
                .await?;

            let mut seen_ids = HashSet::new();
            let mut all = Vec::new();

            let tenant_store = state.store_manager.get_store(&personal_space_id(&auth.tenant_id)).await?;
            for mem in tenant_store.list_all_active().await? {
                if seen_ids.insert(mem.id.clone()) {
                    all.push(mem);
                }
            }

            for space in &spaces {
                if space.id == auth.tenant_id {
                    continue;
                }
                let store = state.store_manager.get_store(&space.id).await?;
                for mem in store.list_all_active().await? {
                    if seen_ids.insert(mem.id.clone()) {
                        all.push(mem);
                    }
                }
            }
            Ok(all)
        }
    }
}

// ── GET /v1/stats ────────────────────────────────────────────────────

pub async fn get_stats(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Query(params): Query<StatsQuery>,
) -> Result<Json<StatsResponse>, OmemError> {
    let memories = collect_memories(&state, &auth, &params.space).await?;

    let total = memories.len();
    let mut by_type: HashMap<String, usize> = HashMap::new();
    let mut by_category: HashMap<String, usize> = HashMap::new();
    let mut by_tier: HashMap<String, usize> = HashMap::new();
    let mut by_state: HashMap<String, usize> = HashMap::new();
    let mut by_space: HashMap<String, usize> = HashMap::new();
    let mut by_visibility: HashMap<String, usize> = HashMap::new();
    let mut by_agent: HashMap<String, usize> = HashMap::new();
    let mut timeline_map: HashMap<String, (usize, HashMap<String, usize>)> = HashMap::new();
    let mut sum_importance = 0.0_f32;
    let mut sum_confidence = 0.0_f32;
    let mut total_access = 0_u32;

    let cutoff_date = chrono::Utc::now() - chrono::Duration::days(i64::from(params.days));
    let cutoff_str = cutoff_date.format("%Y-%m-%d").to_string();

    for mem in &memories {
        *by_type.entry(mem.memory_type.to_string()).or_insert(0) += 1;
        *by_category.entry(mem.category.to_string()).or_insert(0) += 1;
        *by_tier.entry(mem.tier.to_string()).or_insert(0) += 1;
        *by_state.entry(mem.state.to_string()).or_insert(0) += 1;

        let space_key = if mem.space_id.is_empty() {
            "default".to_string()
        } else {
            mem.space_id.clone()
        };
        *by_space.entry(space_key).or_insert(0) += 1;
        *by_visibility.entry(mem.visibility.clone()).or_insert(0) += 1;
        *by_agent.entry(mem.owner_agent_id.clone()).or_insert(0) += 1;

        sum_importance += mem.importance;
        sum_confidence += mem.confidence;
        total_access += mem.access_count;

        let date = if mem.created_at.len() >= 10 {
            &mem.created_at[..10]
        } else {
            &mem.created_at
        };
        if date >= cutoff_str.as_str() {
            let entry = timeline_map
                .entry(date.to_string())
                .or_insert((0, HashMap::new()));
            entry.0 += 1;
            *entry
                .1
                .entry(mem.memory_type.to_string())
                .or_insert(0) += 1;
        }
    }

    let mut timeline: Vec<TimelineEntry> = timeline_map
        .into_iter()
        .map(|(date, (count, by_type))| TimelineEntry {
            date,
            count,
            by_type,
        })
        .collect();
    timeline.sort_by(|a, b| b.date.cmp(&a.date));

    let avg_importance = if total > 0 {
        sum_importance / total as f32
    } else {
        0.0
    };
    let avg_confidence = if total > 0 {
        sum_confidence / total as f32
    } else {
        0.0
    };

    Ok(Json(StatsResponse {
        total,
        by_type,
        by_category,
        by_tier,
        by_state,
        by_space,
        by_visibility,
        by_agent,
        timeline,
        avg_importance,
        avg_confidence,
        total_access_count: total_access,
    }))
}

// ── GET /v1/stats/config ─────────────────────────────────────────────

pub async fn get_config(
    State(_state): State<Arc<AppState>>,
    Extension(_auth): Extension<AuthInfo>,
) -> Result<Json<serde_json::Value>, OmemError> {
    let decay = DecayConfig::default();

    Ok(Json(serde_json::json!({
        "decay": {
            "half_life_days": decay.half_life_days,
            "importance_modulation": decay.importance_modulation,
            "recency_weight": decay.recency_weight,
            "frequency_weight": decay.frequency_weight,
            "intrinsic_weight": decay.intrinsic_weight,
            "tiers": {
                "core": { "beta": decay.beta_core, "floor": decay.floor_core },
                "working": { "beta": decay.beta_working, "floor": decay.floor_working },
                "peripheral": { "beta": decay.beta_peripheral, "floor": decay.floor_peripheral }
            }
        },
        "promotion": {
            "peripheral_to_working": { "min_access_count": 3, "min_composite": 0.4 },
            "working_to_core": { "min_access_count": 10, "min_composite": 0.7, "min_importance": 0.8 }
        },
        "demotion": {
            "core_to_working": { "max_composite": 0.15 },
            "working_to_peripheral": { "max_composite": 0.15 }
        },
        "retrieval": {
            "stages": ["parallel_search","rrf_fusion","rrf_normalize","min_score_filter","topk_cap","cross_encoder_rerank","bm25_floor","decay_boost","importance_weight","length_normalization","hard_cutoff","mmr_diversity"],
            "default_min_score": 0.3,
            "rrf_k": 60,
            "vector_weight": 0.7,
            "bm25_weight": 0.3
        },
        "admission": {
            "presets": {
                "balanced": { "reject": 0.45, "admit": 0.60 },
                "conservative": { "reject": 0.52, "admit": 0.68 },
                "high_recall": { "reject": 0.34, "admit": 0.52 }
            },
            "weights": { "utility": 0.1, "confidence": 0.1, "novelty": 0.1, "recency": 0.1, "type_prior": 0.6 }
        },
        "spaces": {
            "search_weights": { "personal": 1.0, "team": 0.8, "organization": 0.6 },
            "max_spaces_per_user": 20,
            "max_members_per_team": 50
        },
        "categories": ["profile","preferences","entities","events","cases","patterns"],
        "tiers": ["core","working","peripheral"],
        "memory_types": ["pinned","insight","session"],
        "states": ["active","archived","deleted"],
        "relation_types": ["supersedes","contextualizes","supports","contradicts"]
    })))
}

// ── GET /v1/stats/tags ───────────────────────────────────────────────

#[derive(Deserialize)]
pub struct TagsQuery {
    #[serde(default = "default_tag_limit")]
    pub limit: usize,
    #[serde(default = "default_min_count")]
    pub min_count: usize,
    pub space: Option<String>,
}
fn default_tag_limit() -> usize {
    20
}
fn default_min_count() -> usize {
    1
}

pub async fn get_tags(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Query(params): Query<TagsQuery>,
) -> Result<Json<serde_json::Value>, OmemError> {
    let memories = collect_memories(&state, &auth, &params.space).await?;

    let mut tag_counts: HashMap<String, usize> = HashMap::new();
    let mut total_usages = 0_usize;
    let mut tag_spaces: HashMap<String, HashSet<String>> = HashMap::new();

    for mem in &memories {
        let space_key = if mem.space_id.is_empty() {
            "default".to_string()
        } else {
            mem.space_id.clone()
        };
        for tag in &mem.tags {
            *tag_counts.entry(tag.clone()).or_insert(0) += 1;
            total_usages += 1;
            tag_spaces
                .entry(tag.clone())
                .or_default()
                .insert(space_key.clone());
        }
    }

    let cross_space_tags: Vec<String> = tag_spaces
        .iter()
        .filter(|(_, spaces)| spaces.len() >= 2)
        .map(|(tag, _)| tag.clone())
        .collect();

    let mut tags: Vec<(String, usize)> = tag_counts
        .into_iter()
        .filter(|(_, c)| *c >= params.min_count)
        .collect();
    tags.sort_by(|a, b| b.1.cmp(&a.1));
    let total_unique = tags.len();
    tags.truncate(params.limit);

    Ok(Json(serde_json::json!({
        "tags": tags.iter().map(|(name, count)| serde_json::json!({"name": name, "count": count})).collect::<Vec<_>>(),
        "total_unique_tags": total_unique,
        "total_tag_usages": total_usages,
        "cross_space_tags": cross_space_tags,
    })))
}

// ── GET /v1/stats/decay ──────────────────────────────────────────────

#[derive(Deserialize)]
pub struct DecayQuery {
    pub memory_id: String,
    #[serde(default = "default_points")]
    pub points: usize,
}
fn default_points() -> usize {
    90
}

pub async fn get_decay(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Query(params): Query<DecayQuery>,
) -> Result<Json<serde_json::Value>, OmemError> {
    if params.memory_id.is_empty() {
        return Err(OmemError::Validation("memory_id is required".to_string()));
    }

    let store = state.store_manager.get_store(&personal_space_id(&auth.tenant_id)).await?;
    let memory = store
        .get_by_id(&params.memory_id)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("memory {}", params.memory_id)))?;

    let config = DecayConfig::default();
    let engine = DecayEngine::new(DecayConfig::default());

    let (beta, floor) = match memory.tier {
        Tier::Core => (config.beta_core, config.floor_core),
        Tier::Working => (config.beta_working, config.floor_working),
        Tier::Peripheral => (config.beta_peripheral, config.floor_peripheral),
    };

    // Weibull: hl_eff = hl · exp(μ · importance), λ = ln2 / hl_eff
    let effective_hl =
        config.half_life_days * (config.importance_modulation * memory.importance).exp();
    let lambda = (2.0_f32).ln() / effective_hl;

    let current_strength = engine.compute_composite(&memory);

    let now = chrono::Utc::now();
    let last_accessed = memory
        .last_accessed_at
        .as_ref()
        .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
        .map(|dt| dt.with_timezone(&chrono::Utc))
        .unwrap_or_else(|| {
            chrono::DateTime::parse_from_rfc3339(&memory.created_at)
                .map(|dt| dt.with_timezone(&chrono::Utc))
                .unwrap_or(now)
        });
    let last_accessed_hours_ago = (now - last_accessed).num_minutes() as f32 / 60.0;

    let mut decay_curve = Vec::new();
    for i in 0..params.points {
        let day = i as f32;
        let strength = (-lambda * day.powf(beta)).exp();
        let floored = strength.max(floor);
        decay_curve.push(serde_json::json!({"day": i, "strength": floored}));
    }

    Ok(Json(serde_json::json!({
        "memory_id": memory.id,
        "tier": memory.tier.to_string(),
        "importance": memory.importance,
        "confidence": memory.confidence,
        "access_count": memory.access_count,
        "last_accessed_hours_ago": last_accessed_hours_ago,
        "current_strength": current_strength,
        "composite_score": current_strength,
        "decay_params": {
            "beta": beta,
            "half_life_days": effective_hl,
            "lambda": lambda,
            "floor": floor,
            "importance_modulation": config.importance_modulation
        },
        "decay_curve": decay_curve,
        "promotion_thresholds": {
            "to_working": { "access_count": 3, "composite": 0.4 },
            "to_core": { "access_count": 10, "composite": 0.7, "importance": 0.8 }
        },
        "demotion_thresholds": {
            "core_to_working": { "composite": 0.15 },
            "working_to_peripheral": { "composite": 0.15 }
        }
    })))
}

// ── GET /v1/stats/relations ──────────────────────────────────────────

#[derive(Deserialize)]
pub struct RelationsQuery {
    #[serde(default = "default_relation_limit")]
    pub limit: usize,
    #[serde(default)]
    pub min_importance: f32,
}
fn default_relation_limit() -> usize {
    100
}

pub async fn get_relations(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Query(params): Query<RelationsQuery>,
) -> Result<Json<serde_json::Value>, OmemError> {
    let memories = collect_memories(&state, &auth, &None).await?;

    let filtered: Vec<_> = memories
        .into_iter()
        .filter(|m| m.importance >= params.min_importance)
        .collect();

    let mut node_ids = HashSet::new();
    let mut edges = Vec::new();

    let mem_space_map: HashMap<String, String> = filtered
        .iter()
        .map(|m| (m.id.clone(), m.space_id.clone()))
        .collect();

    for mem in &filtered {
        for rel in &mem.relations {
            node_ids.insert(mem.id.clone());
            node_ids.insert(rel.target_id.clone());
            let source_space = &mem.space_id;
            let target_space = mem_space_map
                .get(&rel.target_id)
                .cloned()
                .unwrap_or_default();
            let is_cross_space =
                !source_space.is_empty() && !target_space.is_empty() && source_space != &target_space;
            edges.push(serde_json::json!({
                "source": mem.id,
                "target": rel.target_id,
                "relation_type": rel.relation_type.to_string(),
                "context_label": rel.context_label,
                "cross_space": is_cross_space
            }));
        }
        if let Some(ref sup) = mem.superseded_by {
            node_ids.insert(mem.id.clone());
            node_ids.insert(sup.clone());
            edges.push(serde_json::json!({
                "source": sup,
                "target": mem.id,
                "relation_type": "supersedes",
                "context_label": null,
                "cross_space": false
            }));
        }
        if let Some(ref prov) = mem.provenance {
            node_ids.insert(mem.id.clone());
            node_ids.insert(prov.shared_from_memory.clone());
            edges.push(serde_json::json!({
                "source": prov.shared_from_memory,
                "target": mem.id,
                "relation_type": "shared_from",
                "context_label": format!("{} → {}", prov.shared_from_space, mem.space_id),
                "cross_space": prov.shared_from_space != mem.space_id
            }));
        }
    }

    let nodes: Vec<serde_json::Value> = filtered
        .iter()
        .filter(|m| node_ids.contains(&m.id))
        .take(params.limit)
        .map(|m| {
            serde_json::json!({
                "id": m.id,
                "label": if m.l0_abstract.is_empty() { &m.content } else { &m.l0_abstract },
                "category": m.category.to_string(),
                "tier": m.tier.to_string(),
                "importance": m.importance,
                "access_count": m.access_count,
                "memory_type": m.memory_type.to_string(),
                "space_id": m.space_id
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "nodes": nodes,
        "edges": edges,
        "total_nodes": nodes.len(),
        "total_edges": edges.len()
    })))
}

// ── GET /v1/stats/spaces — Space overview ────────────────────────────

pub async fn get_spaces_stats(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
) -> Result<Json<serde_json::Value>, OmemError> {
    let spaces = state
        .space_store
        .list_spaces_for_user(&auth.tenant_id)
        .await?;

    let mut space_stats = Vec::new();

    for space in &spaces {
        let store = state.store_manager.get_store(&space.id).await?;
        let memories = store.list_all_active().await?;

        let memory_count = memories.len();
        let agent_ids: HashSet<&str> = memories.iter().map(|m| m.owner_agent_id.as_str()).collect();
        let agent_count = agent_ids.len();

        let mut tier_distribution: HashMap<String, usize> = HashMap::new();
        let mut category_counts: HashMap<String, usize> = HashMap::new();
        let mut last_activity: Option<&str> = None;
        let mut shared_in_count = 0_usize;

        for mem in &memories {
            *tier_distribution
                .entry(mem.tier.to_string())
                .or_insert(0) += 1;
            *category_counts
                .entry(mem.category.to_string())
                .or_insert(0) += 1;

            let ts = mem.created_at.as_str();
            if last_activity.is_none() || ts > last_activity.unwrap_or_default() {
                last_activity = Some(ts);
            }
            if mem.provenance.is_some() {
                shared_in_count += 1;
            }
        }

        let mut top_categories: Vec<(String, usize)> = category_counts.into_iter().collect();
        top_categories.sort_by(|a, b| b.1.cmp(&a.1));
        top_categories.truncate(3);

        space_stats.push(serde_json::json!({
            "space_id": space.id,
            "space_type": space.space_type.to_string(),
            "name": space.name,
            "owner_id": space.owner_id,
            "memory_count": memory_count,
            "agent_count": agent_count,
            "tier_distribution": tier_distribution,
            "top_categories": top_categories.iter().map(|(c, n)| serde_json::json!({"category": c, "count": n})).collect::<Vec<_>>(),
            "last_activity": last_activity,
            "shared_in_count": shared_in_count,
            "member_count": space.members.len(),
            "members": space.members.iter().map(|m| serde_json::json!({"user_id": m.user_id, "role": m.role.to_string()})).collect::<Vec<_>>(),
        }));
    }

    Ok(Json(serde_json::json!({
        "spaces": space_stats,
        "total_spaces": spaces.len(),
    })))
}

// ── GET /v1/stats/sharing — Sharing flow analysis ────────────────────

#[derive(Deserialize)]
pub struct SharingStatsQuery {
    #[serde(default = "default_days")]
    pub days: u32,
}

pub async fn get_sharing_stats(
    State(state): State<Arc<AppState>>,
    Extension(_auth): Extension<AuthInfo>,
    Query(params): Query<SharingStatsQuery>,
) -> Result<Json<serde_json::Value>, OmemError> {
    let limit = (params.days as usize) * 100;
    let events = state.space_store.list_all_events(limit).await?;

    let cutoff_date = chrono::Utc::now() - chrono::Duration::days(i64::from(params.days));
    let cutoff_str = cutoff_date.to_rfc3339();

    let recent_events: Vec<_> = events
        .into_iter()
        .filter(|e| e.timestamp >= cutoff_str)
        .collect();

    let mut total_shares = 0_usize;
    let mut total_pulls = 0_usize;
    let mut total_unshares = 0_usize;
    let mut unique_sharers: HashSet<String> = HashSet::new();
    let mut flow_edges: HashMap<(String, String), usize> = HashMap::new();
    let mut daily_counts: HashMap<String, (usize, usize)> = HashMap::new();

    for event in &recent_events {
        match event.action {
            SharingAction::Share | SharingAction::BatchShare => {
                total_shares += 1;
                unique_sharers.insert(event.user_id.clone());
                *flow_edges
                    .entry((event.from_space.clone(), event.to_space.clone()))
                    .or_insert(0) += 1;
            }
            SharingAction::Pull => {
                total_pulls += 1;
            }
            SharingAction::Unshare => {
                total_unshares += 1;
            }
        }

        let date = if event.timestamp.len() >= 10 {
            &event.timestamp[..10]
        } else {
            &event.timestamp
        };
        let entry = daily_counts.entry(date.to_string()).or_insert((0, 0));
        match event.action {
            SharingAction::Share | SharingAction::BatchShare => entry.0 += 1,
            SharingAction::Pull => entry.1 += 1,
            _ => {}
        }
    }

    let mut flow_nodes: HashSet<String> = HashSet::new();
    let flow_edges_json: Vec<serde_json::Value> = flow_edges
        .iter()
        .map(|((from, to), count)| {
            flow_nodes.insert(from.clone());
            flow_nodes.insert(to.clone());
            serde_json::json!({
                "from": from,
                "to": to,
                "count": count
            })
        })
        .collect();

    let mut timeline: Vec<serde_json::Value> = daily_counts
        .into_iter()
        .map(|(date, (shares, pulls))| {
            serde_json::json!({
                "date": date,
                "shares": shares,
                "pulls": pulls
            })
        })
        .collect();
    timeline.sort_by(|a, b| {
        let da = a["date"].as_str().unwrap_or_default();
        let db = b["date"].as_str().unwrap_or_default();
        db.cmp(da)
    });

    let recent_activity: Vec<serde_json::Value> = recent_events
        .iter()
        .rev()
        .take(20)
        .map(|e| {
            serde_json::json!({
                "id": e.id,
                "action": e.action.to_string(),
                "memory_id": e.memory_id,
                "from_space": e.from_space,
                "to_space": e.to_space,
                "user_id": e.user_id,
                "agent_id": e.agent_id,
                "content_preview": e.content_preview,
                "timestamp": e.timestamp,
            })
        })
        .collect();

    Ok(Json(serde_json::json!({
        "summary": {
            "total_shares": total_shares,
            "total_pulls": total_pulls,
            "total_unshares": total_unshares,
            "unique_sharers": unique_sharers.len(),
        },
        "recent_activity": recent_activity,
        "flow_graph": {
            "nodes": flow_nodes.iter().collect::<Vec<_>>(),
            "edges": flow_edges_json,
        },
        "timeline": timeline,
    })))
}

// ── GET /v1/stats/agents — Agent activity ────────────────────────────

pub async fn get_agents_stats(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
) -> Result<Json<serde_json::Value>, OmemError> {
    let memories = collect_memories(&state, &auth, &None).await?;

    let mut agent_map: HashMap<String, Vec<&Memory>> = HashMap::new();
    for mem in &memories {
        agent_map
            .entry(mem.owner_agent_id.clone())
            .or_default()
            .push(mem);
    }

    let events = state.space_store.list_all_events(10000).await?;
    let mut agent_share_counts: HashMap<String, usize> = HashMap::new();
    for event in &events {
        if matches!(
            event.action,
            SharingAction::Share | SharingAction::BatchShare
        ) {
            *agent_share_counts
                .entry(event.agent_id.clone())
                .or_insert(0) += 1;
        }
    }

    let mut agents = Vec::new();
    for (agent_id, mems) in &agent_map {
        let total_memories = mems.len();

        let mut by_space: HashMap<String, usize> = HashMap::new();
        let mut category_counts: HashMap<String, usize> = HashMap::new();
        let mut last_active: Option<&str> = None;

        for mem in mems {
            let space_key = if mem.space_id.is_empty() {
                "default".to_string()
            } else {
                mem.space_id.clone()
            };
            *by_space.entry(space_key).or_insert(0) += 1;
            *category_counts
                .entry(mem.category.to_string())
                .or_insert(0) += 1;

            let ts = mem.created_at.as_str();
            if last_active.is_none() || ts > last_active.unwrap_or_default() {
                last_active = Some(ts);
            }
        }

        let mut top_categories: Vec<(String, usize)> = category_counts.into_iter().collect();
        top_categories.sort_by(|a, b| b.1.cmp(&a.1));
        top_categories.truncate(3);

        let share_count = agent_share_counts.get(agent_id).copied().unwrap_or(0);

        agents.push(serde_json::json!({
            "agent_id": agent_id,
            "total_memories": total_memories,
            "memories_by_space": by_space,
            "top_categories": top_categories.iter().map(|(c, n)| serde_json::json!({"category": c, "count": n})).collect::<Vec<_>>(),
            "last_active": last_active,
            "share_count": share_count,
        }));
    }

    agents.sort_by(|a, b| {
        let ca = a["total_memories"].as_u64().unwrap_or(0);
        let cb = b["total_memories"].as_u64().unwrap_or(0);
        cb.cmp(&ca)
    });

    Ok(Json(serde_json::json!({
        "agents": agents,
        "total_agents": agent_map.len(),
    })))
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::OmemConfig;
    use crate::domain::category::Category;
    use crate::domain::memory::Memory;
    use crate::domain::space::{
        MemberRole, Provenance, SharingAction, SharingEvent, Space, SpaceMember, SpaceType,
    };
    use crate::domain::types::MemoryType;
    use crate::embed::NoopEmbedder;
    use crate::llm::NoopLlm;
    use crate::store::{SpaceStore, StoreManager, TenantStore};
    use tempfile::TempDir;

    async fn setup_state() -> (Arc<AppState>, TempDir, TempDir, TempDir) {
        let store_dir = TempDir::new().expect("temp dir");
        let space_dir = TempDir::new().expect("temp dir");
        let tenant_dir = TempDir::new().expect("temp dir");

        let store_manager = Arc::new(StoreManager::new(
            store_dir.path().to_str().expect("path"),
        ));
        let space_store = Arc::new(
            SpaceStore::new(space_dir.path().to_str().expect("path"))
                .await
                .expect("space store"),
        );
        space_store.init_tables().await.expect("init tables");
        let tenant_store = Arc::new(
            TenantStore::new(tenant_dir.path().to_str().expect("path"))
                .await
                .expect("tenant store"),
        );
        tenant_store.init_table().await.expect("init tenant");

        let state = Arc::new(AppState {
            store_manager,
            tenant_store,
            space_store,
            embed: Arc::new(NoopEmbedder::new(1024)),
            llm: Arc::new(NoopLlm),
            config: OmemConfig::default(),
        });

        (state, store_dir, space_dir, tenant_dir)
    }

    fn make_auth(tenant_id: &str) -> AuthInfo {
        AuthInfo {
            tenant_id: tenant_id.to_string(),
            agent_id: None,
        }
    }

    fn make_space(id: &str, name: &str, owner: &str, space_type: SpaceType) -> Space {
        Space {
            id: id.to_string(),
            space_type,
            name: name.to_string(),
            owner_id: owner.to_string(),
            members: vec![SpaceMember {
                user_id: owner.to_string(),
                role: MemberRole::Admin,
                joined_at: "2025-01-01T00:00:00Z".to_string(),
            }],
            auto_share_rules: Vec::new(),
            created_at: "2025-01-01T00:00:00Z".to_string(),
            updated_at: "2025-01-01T00:00:00Z".to_string(),
        }
    }

    fn make_memory(content: &str, space_id: &str, agent_id: &str) -> Memory {
        let mut mem = Memory::new(content, Category::Preferences, MemoryType::Insight, "test");
        mem.space_id = space_id.to_string();
        mem.owner_agent_id = agent_id.to_string();
        mem
    }

    // ── test_stats_by_space ──

    #[tokio::test]
    async fn test_stats_by_space() {
        let (state, _d1, _d2, _d3) = setup_state().await;
        let auth = make_auth("user-1");

        let s1 = make_space("personal:user-1", "Personal", "user-1", SpaceType::Personal);
        let s2 = make_space("team:backend", "Backend", "user-1", SpaceType::Team);
        state.space_store.create_space(&s1).await.expect("create s1");
        state.space_store.create_space(&s2).await.expect("create s2");

        let store1 = state.store_manager.get_store("personal:user-1").await.expect("store1");
        let store2 = state.store_manager.get_store("team:backend").await.expect("store2");

        let m1 = make_memory("personal mem", "personal:user-1", "coder");
        let m2 = make_memory("team mem 1", "team:backend", "coder");
        let m3 = make_memory("team mem 2", "team:backend", "writer");
        store1.create(&m1, None).await.expect("create m1");
        store2.create(&m2, None).await.expect("create m2");
        store2.create(&m3, None).await.expect("create m3");

        let result = get_stats(
            State(state),
            Extension(auth),
            Query(StatsQuery { days: 30, space: None }),
        )
        .await
        .expect("get_stats");

        let resp = result.0;
        assert_eq!(resp.total, 3);
        assert_eq!(resp.by_space.get("personal:user-1"), Some(&1));
        assert_eq!(resp.by_space.get("team:backend"), Some(&2));
        assert_eq!(resp.by_agent.get("coder"), Some(&2));
        assert_eq!(resp.by_agent.get("writer"), Some(&1));
    }

    // ── test_stats_space_filter ──

    #[tokio::test]
    async fn test_stats_space_filter() {
        let (state, _d1, _d2, _d3) = setup_state().await;
        let auth = make_auth("user-1");

        let s1 = make_space("personal:user-1", "Personal", "user-1", SpaceType::Personal);
        let s2 = make_space("team:backend", "Backend", "user-1", SpaceType::Team);
        state.space_store.create_space(&s1).await.expect("s1");
        state.space_store.create_space(&s2).await.expect("s2");

        let store1 = state.store_manager.get_store("personal:user-1").await.expect("store1");
        let store2 = state.store_manager.get_store("team:backend").await.expect("store2");
        store1.create(&make_memory("p1", "personal:user-1", "a1"), None).await.expect("m1");
        store2.create(&make_memory("t1", "team:backend", "a1"), None).await.expect("m2");
        store2.create(&make_memory("t2", "team:backend", "a2"), None).await.expect("m3");

        let result = get_stats(
            State(state),
            Extension(auth),
            Query(StatsQuery {
                days: 30,
                space: Some("personal:user-1".to_string()),
            }),
        )
        .await
        .expect("get_stats filtered");

        let resp = result.0;
        assert_eq!(resp.total, 1);
        assert_eq!(resp.by_space.get("personal:user-1"), Some(&1));
        assert!(!resp.by_space.contains_key("team:backend"));
    }

    // ── test_spaces_stats ──

    #[tokio::test]
    async fn test_spaces_stats() {
        let (state, _d1, _d2, _d3) = setup_state().await;
        let auth = make_auth("user-1");

        let s1 = make_space("personal:user-1", "Personal", "user-1", SpaceType::Personal);
        state.space_store.create_space(&s1).await.expect("s1");

        let store1 = state.store_manager.get_store("personal:user-1").await.expect("store1");
        store1.create(&make_memory("mem1", "personal:user-1", "coder"), None).await.expect("m1");
        store1.create(&make_memory("mem2", "personal:user-1", "writer"), None).await.expect("m2");

        let result = get_spaces_stats(State(state), Extension(auth))
            .await
            .expect("spaces stats");

        let body = result.0;
        assert_eq!(body["total_spaces"], 1);
        let spaces = body["spaces"].as_array().expect("spaces array");
        assert_eq!(spaces.len(), 1);
        assert_eq!(spaces[0]["space_id"], "personal:user-1");
        assert_eq!(spaces[0]["memory_count"], 2);
        assert_eq!(spaces[0]["agent_count"], 2);
    }

    // ── test_sharing_stats ──

    #[tokio::test]
    async fn test_sharing_stats() {
        let (state, _d1, _d2, _d3) = setup_state().await;
        let auth = make_auth("user-1");

        let event = SharingEvent {
            id: "evt-1".to_string(),
            action: SharingAction::Share,
            memory_id: "mem-1".to_string(),
            from_space: "personal:user-1".to_string(),
            to_space: "team:backend".to_string(),
            user_id: "user-1".to_string(),
            agent_id: "coder".to_string(),
            content_preview: "dark mode pref".to_string(),
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        state
            .space_store
            .record_sharing_event(&event)
            .await
            .expect("record event");

        let result = get_sharing_stats(
            State(state),
            Extension(auth),
            Query(SharingStatsQuery { days: 30 }),
        )
        .await
        .expect("sharing stats");

        let body = result.0;
        assert_eq!(body["summary"]["total_shares"], 1);
        assert_eq!(body["summary"]["unique_sharers"], 1);
        let activity = body["recent_activity"].as_array().expect("array");
        assert_eq!(activity.len(), 1);
        assert_eq!(activity[0]["memory_id"], "mem-1");
    }

    // ── test_agents_stats ──

    #[tokio::test]
    async fn test_agents_stats() {
        let (state, _d1, _d2, _d3) = setup_state().await;
        let auth = make_auth("user-1");

        let s1 = make_space("personal:user-1", "Personal", "user-1", SpaceType::Personal);
        state.space_store.create_space(&s1).await.expect("s1");

        let store = state.store_manager.get_store("personal:user-1").await.expect("store");
        store.create(&make_memory("m1", "personal:user-1", "coder"), None).await.expect("m1");
        store.create(&make_memory("m2", "personal:user-1", "coder"), None).await.expect("m2");
        store.create(&make_memory("m3", "personal:user-1", "writer"), None).await.expect("m3");

        let result = get_agents_stats(State(state), Extension(auth))
            .await
            .expect("agents stats");

        let body = result.0;
        assert_eq!(body["total_agents"], 2);
        let agents = body["agents"].as_array().expect("array");
        assert_eq!(agents.len(), 2);
        assert_eq!(agents[0]["agent_id"], "coder");
        assert_eq!(agents[0]["total_memories"], 2);
        assert_eq!(agents[1]["agent_id"], "writer");
        assert_eq!(agents[1]["total_memories"], 1);
    }

    // ── test_tags_cross_space ──

    #[tokio::test]
    async fn test_tags_cross_space() {
        let (state, _d1, _d2, _d3) = setup_state().await;
        let auth = make_auth("user-1");

        let s1 = make_space("personal:user-1", "Personal", "user-1", SpaceType::Personal);
        let s2 = make_space("team:backend", "Backend", "user-1", SpaceType::Team);
        state.space_store.create_space(&s1).await.expect("s1");
        state.space_store.create_space(&s2).await.expect("s2");

        let store1 = state.store_manager.get_store("personal:user-1").await.expect("store1");
        let store2 = state.store_manager.get_store("team:backend").await.expect("store2");

        let mut m1 = make_memory("m1", "personal:user-1", "coder");
        m1.tags = vec!["rust".to_string(), "personal-only".to_string()];
        let mut m2 = make_memory("m2", "team:backend", "coder");
        m2.tags = vec!["rust".to_string(), "team-only".to_string()];

        store1.create(&m1, None).await.expect("m1");
        store2.create(&m2, None).await.expect("m2");

        let result = get_tags(
            State(state),
            Extension(auth),
            Query(TagsQuery {
                limit: 20,
                min_count: 1,
                space: None,
            }),
        )
        .await
        .expect("tags");

        let body = result.0;
        let cross = body["cross_space_tags"]
            .as_array()
            .expect("cross_space_tags array");
        let cross_strs: Vec<&str> = cross.iter().filter_map(|v| v.as_str()).collect();
        assert!(cross_strs.contains(&"rust"));
        assert!(!cross_strs.contains(&"personal-only"));
        assert!(!cross_strs.contains(&"team-only"));
    }

    // ── test_relations_cross_space ──

    #[tokio::test]
    async fn test_relations_cross_space() {
        let (state, _d1, _d2, _d3) = setup_state().await;
        let auth = make_auth("user-1");

        let s1 = make_space("personal:user-1", "Personal", "user-1", SpaceType::Personal);
        state.space_store.create_space(&s1).await.expect("s1");

        let store = state.store_manager.get_store("personal:user-1").await.expect("store");

        let m1 = make_memory("original", "personal:user-1", "coder");
        let original_id = m1.id.clone();
        store.create(&m1, None).await.expect("m1");

        let mut m2 = make_memory("shared copy", "team:backend", "coder");
        m2.provenance = Some(Provenance {
            shared_from_space: "personal:user-1".to_string(),
            shared_from_memory: original_id.clone(),
            shared_by_user: "user-1".to_string(),
            shared_by_agent: "coder".to_string(),
            shared_at: chrono::Utc::now().to_rfc3339(),
            original_created_at: m1.created_at.clone(),
        });
        store.create(&m2, None).await.expect("m2");

        let result = get_relations(
            State(state),
            Extension(auth),
            Query(RelationsQuery {
                limit: 100,
                min_importance: 0.0,
            }),
        )
        .await
        .expect("relations");

        let body = result.0;
        let edges = body["edges"].as_array().expect("edges array");
        let shared_edge = edges
            .iter()
            .find(|e| e["relation_type"] == "shared_from")
            .expect("should have shared_from edge");
        assert_eq!(shared_edge["source"], original_id);
        assert_eq!(shared_edge["cross_space"], true);
    }

    // ── test_config_spaces ──

    #[tokio::test]
    async fn test_config_spaces() {
        let (state, _d1, _d2, _d3) = setup_state().await;
        let auth = make_auth("user-1");

        let result = get_config(State(state), Extension(auth))
            .await
            .expect("config");

        let body = result.0;
        assert!(body["spaces"].is_object());
        assert_eq!(body["spaces"]["search_weights"]["personal"], 1.0);
        assert_eq!(body["spaces"]["search_weights"]["team"], 0.8);
        assert_eq!(body["spaces"]["search_weights"]["organization"], 0.6);
        assert_eq!(body["spaces"]["max_spaces_per_user"], 20);
        assert_eq!(body["spaces"]["max_members_per_team"], 50);
    }
}
