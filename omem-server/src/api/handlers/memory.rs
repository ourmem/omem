use std::sync::Arc;

use axum::extract::{Extension, Path, Query, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use crate::api::server::{AppState, personal_space_id};
use crate::domain::category::Category;
use crate::domain::error::OmemError;
use crate::domain::memory::Memory;
use crate::domain::tenant::AuthInfo;
use crate::domain::types::MemoryType;
use crate::ingest::types::{IngestMessage, IngestMode, IngestRequest};
use crate::ingest::IngestPipeline;
use crate::ingest::SessionStore;
use crate::retrieve::pipeline::SearchRequest;
use crate::retrieve::RetrievalPipeline;
use crate::store::lancedb::ListFilter;
use crate::store::StoreManager;

// ── Request / Response DTOs ──────────────────────────────────────────

#[derive(Deserialize)]
pub struct CreateMemoryBody {
    // Message-based ingest
    pub messages: Option<Vec<MessageDto>>,
    #[serde(default)]
    pub mode: Option<String>,
    pub agent_id: Option<String>,
    pub session_id: Option<String>,
    pub entity_context: Option<String>,

    // Direct single memory creation
    pub content: Option<String>,
    #[serde(default)]
    pub tags: Option<Vec<String>>,
    pub source: Option<String>,
}

#[derive(Deserialize)]
pub struct MessageDto {
    pub role: String,
    pub content: String,
}

#[derive(Deserialize)]
pub struct SearchQuery {
    pub q: String,
    #[serde(default = "default_limit")]
    pub limit: usize,
    pub scope: Option<String>,
    pub min_score: Option<f32>,
    #[serde(default)]
    pub include_trace: bool,
    pub space: Option<String>,
    pub tags: Option<String>,
    pub source: Option<String>,
    pub agent_id: Option<String>,
    #[serde(default)]
    pub check_stale: bool,
}

fn default_limit() -> usize {
    20
}

#[derive(Deserialize)]
pub struct ListQuery {
    #[serde(default = "default_limit")]
    pub limit: usize,
    #[serde(default)]
    pub offset: usize,
    pub memory_type: Option<String>,
    pub state: Option<String>,
    pub category: Option<String>,
    pub tier: Option<String>,
    pub tags: Option<String>,
    #[serde(default = "default_sort")]
    pub sort: String,
    #[serde(default = "default_order")]
    pub order: String,
}

fn default_sort() -> String {
    "created_at".to_string()
}
fn default_order() -> String {
    "desc".to_string()
}

#[derive(Deserialize)]
pub struct UpdateMemoryBody {
    pub content: Option<String>,
    pub tags: Option<Vec<String>>,
    pub state: Option<String>,
}

#[derive(Serialize)]
pub struct SearchResultDto {
    pub memory: Memory,
    pub score: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stale_info: Option<StaleInfo>,
}

#[derive(Serialize)]
pub struct SearchResponseDto {
    pub results: Vec<SearchResultDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<serde_json::Value>,
}

#[derive(Serialize, Clone)]
pub struct StaleInfo {
    pub is_stale: bool,
    pub source_version: Option<u64>,
    pub current_source_version: Option<u64>,
    pub source_deleted: bool,
}

#[derive(Deserialize)]
pub struct GetMemoryQuery {
    #[serde(default)]
    pub check_stale: bool,
}

#[derive(Serialize)]
pub struct ListResponseDto {
    pub memories: Vec<Memory>,
    pub total_count: usize,
    pub limit: usize,
    pub offset: usize,
}

// ── Handlers ─────────────────────────────────────────────────────────

/// POST /v1/memories
///
/// Two modes:
/// - If `messages` present → ingest pipeline (async), returns 202
/// - If `content` present → create single pinned memory, returns 201
pub async fn create_memory(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Json(body): Json<CreateMemoryBody>,
) -> Result<impl IntoResponse, OmemError> {
    let store = state.store_manager.get_store(&personal_space_id(&auth.tenant_id)).await?;

    if let Some(messages) = body.messages {
        if messages.is_empty() {
            return Err(OmemError::Validation("messages array is empty".to_string()));
        }

        let mode = match body.mode.as_deref() {
            Some("raw") => IngestMode::Raw,
            _ => IngestMode::Smart,
        };

        let session_uri = format!("{}/{}", state.config.store_uri(), personal_space_id(&auth.tenant_id));

        let request = IngestRequest {
            messages: messages
                .into_iter()
                .map(|m| IngestMessage {
                    role: m.role,
                    content: m.content,
                })
                .collect(),
            tenant_id: auth.tenant_id,
            agent_id: body.agent_id.or(auth.agent_id),
            session_id: body.session_id,
            entity_context: body.entity_context,
            mode,
        };

        let session_store = Arc::new(
            SessionStore::new(&session_uri)
                .await
                .map_err(|e| OmemError::Storage(format!("session store: {e}")))?,
        );
        session_store.init_table().await?;

        let ingest_pipeline = IngestPipeline::new(
            store,
            session_store,
            state.embed.clone(),
            state.llm.clone(),
        );

        let response = ingest_pipeline.ingest(request).await?;
        return Ok((StatusCode::ACCEPTED, Json(serde_json::json!(response))).into_response());
    }

    let content = body
        .content
        .ok_or_else(|| OmemError::Validation("either 'messages' or 'content' required".to_string()))?;

    if content.is_empty() {
        return Err(OmemError::Validation("content cannot be empty".to_string()));
    }

    let mut memory = Memory::new(
        &content,
        Category::Preferences,
        MemoryType::Pinned,
        &auth.tenant_id,
    );
    memory.tags = body.tags.unwrap_or_default();
    memory.source = body.source;
    memory.agent_id = auth.agent_id;

    let vectors = state
        .embed
        .embed(&[content])
        .await
        .map_err(|e| OmemError::Embedding(format!("failed to embed content: {e}")))?;
    let vector = vectors.into_iter().next();

    store
        .create(&memory, vector.as_deref())
        .await?;

    // Fire-and-forget: check auto-share rules for the newly created memory
    {
        let as_memory = memory.clone();
        let as_user = auth.tenant_id.clone();
        let as_agent = as_memory.agent_id.clone().unwrap_or_default();
        let as_space_store = state.space_store.clone();
        let as_store_mgr = state.store_manager.clone();
        tokio::spawn(async move {
            if let Err(e) = super::sharing::check_auto_share(
                &as_memory,
                &as_space_store,
                &as_store_mgr,
                &as_user,
                &as_agent,
            )
            .await
            {
                tracing::warn!(
                    memory_id = %as_memory.id,
                    error = %e,
                    "auto-share check failed (non-fatal)"
                );
            }
        });
    }

    Ok((StatusCode::CREATED, Json(serde_json::json!(memory))).into_response())
}

/// GET /v1/memories/search
pub async fn search_memories(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Query(params): Query<SearchQuery>,
) -> Result<Json<SearchResponseDto>, OmemError> {
    if params.q.is_empty() {
        return Err(OmemError::Validation("query parameter 'q' is required".to_string()));
    }

    let vectors = state
        .embed
        .embed(std::slice::from_ref(&params.q))
        .await
        .map_err(|e| OmemError::Embedding(format!("failed to embed query: {e}")))?;
    let query_vector = vectors.into_iter().next();

    let spaces = state
        .space_store
        .list_spaces_for_user(&auth.tenant_id)
        .await?;

    if spaces.is_empty() {
        let store = state.store_manager.get_store(&personal_space_id(&auth.tenant_id)).await?;

        let request = SearchRequest {
            query: params.q,
            query_vector,
            tenant_id: auth.tenant_id,
            scope_filter: params.scope,
            limit: Some(params.limit),
            min_score: params.min_score,
            include_trace: params.include_trace,
            tags_filter: params.tags.as_ref().map(|t| t.split(',').map(|s| s.trim().to_string()).collect()),
            source_filter: params.source.clone(),
            agent_id_filter: params.agent_id.clone(),
        };

        let retrieval_pipeline = RetrievalPipeline::new(store);
        let search_results = retrieval_pipeline.search(&request).await?;

        let mut results: Vec<SearchResultDto> = search_results
            .results
            .into_iter()
            .map(|r| SearchResultDto {
                memory: r.memory,
                score: r.score,
                stale_info: None,
            })
            .collect();

        if params.check_stale {
            for result in &mut results {
                result.stale_info = check_stale_for_memory(&result.memory, &state.store_manager).await;
            }
        }

        let trace = build_trace(params.include_trace, &search_results.trace);
        return Ok(Json(SearchResponseDto { results, trace }));
    }

    let target_spaces: Vec<_> = if let Some(ref space_param) = params.space {
        if space_param == "all" {
            spaces
        } else {
            let requested: Vec<&str> = space_param.split(',').map(|s| s.trim()).collect();
            spaces
                .into_iter()
                .filter(|s| requested.contains(&s.id.as_str()))
                .collect()
        }
    } else {
        spaces
    };

    let accessible = state
        .store_manager
        .get_accessible_stores(&auth.tenant_id, &target_spaces)
        .await?;

    // Parallel cross-space search via JoinSet
    let mut join_set = tokio::task::JoinSet::new();
    for acc in accessible {
        let query = params.q.clone();
        let query_vector = query_vector.clone();
        let tenant_id = auth.tenant_id.clone();
        let scope_filter = params.scope.clone();
        let limit = params.limit;
        let min_score = params.min_score;
        let tags_filter = params.tags.as_ref().map(|t| {
            t.split(',').map(|s| s.trim().to_string()).collect::<Vec<_>>()
        });
        let source_filter = params.source.clone();
        let agent_id_filter = params.agent_id.clone();
        let store = acc.store.clone();
        let space_id = acc.space_id.clone();
        let weight = acc.weight;

        join_set.spawn(async move {
            let request = SearchRequest {
                query,
                query_vector,
                tenant_id,
                scope_filter,
                limit: Some(limit),
                min_score,
                include_trace: false,
                tags_filter,
                source_filter,
                agent_id_filter,
            };
            let pipeline = RetrievalPipeline::new(store);
            let result = pipeline.search(&request).await;
            (space_id, weight, result)
        });
    }

    let mut all_results: Vec<(Memory, f32, String)> = Vec::new();
    while let Some(join_result) = join_set.join_next().await {
        match join_result {
            Ok((space_id, weight, Ok(search_results))) => {
                let max_score = search_results
                    .results
                    .iter()
                    .map(|r| r.score)
                    .fold(0.0_f32, f32::max);

                for r in search_results.results {
                    let normalized = if max_score > 0.0 {
                        r.score / max_score
                    } else {
                        0.0
                    };
                    let weighted = normalized * weight;
                    all_results.push((r.memory, weighted, space_id.clone()));
                }
            }
            Ok((space_id, _, Err(e))) => {
                tracing::warn!(space_id = %space_id, error = %e, "cross-space search failed for space, skipping");
            }
            Err(e) => {
                tracing::warn!(error = %e, "join error in cross-space search");
            }
        }
    }

    all_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    all_results.truncate(params.limit);

    let mut results: Vec<SearchResultDto> = all_results
        .into_iter()
        .map(|(memory, score, _space_id)| SearchResultDto { memory, score, stale_info: None })
        .collect();

    if params.check_stale {
        for result in &mut results {
            result.stale_info = check_stale_for_memory(&result.memory, &state.store_manager).await;
        }
    }

    Ok(Json(SearchResponseDto {
        results,
        trace: None,
    }))
}

fn build_trace(include: bool, trace: &crate::retrieve::trace::RetrievalTrace) -> Option<serde_json::Value> {
    if !include {
        return None;
    }
    Some(serde_json::json!({
        "stages": trace.stages.iter().map(|s| {
            serde_json::json!({
                "name": s.name,
                "input_count": s.input_count,
                "output_count": s.output_count,
                "duration_ms": s.duration_ms,
                "score_range": s.score_range,
            })
        }).collect::<Vec<_>>(),
        "total_duration_ms": trace.total_duration_ms,
        "final_count": trace.final_count,
    }))
}

pub(crate) async fn check_stale_for_memory(memory: &Memory, store_manager: &StoreManager) -> Option<StaleInfo> {
    let provenance = memory.provenance.as_ref()?;

    let source_store = store_manager
        .get_store(&provenance.shared_from_space)
        .await
        .ok()?;

    match source_store.get_by_id(&provenance.shared_from_memory).await {
        Ok(Some(source)) => {
            let source_ver = provenance.source_version.unwrap_or(0);
            let current_ver = source.version.unwrap_or(0);
            Some(StaleInfo {
                is_stale: source_ver < current_ver,
                source_version: provenance.source_version,
                current_source_version: source.version,
                source_deleted: false,
            })
        }
        Ok(None) => Some(StaleInfo {
            is_stale: true,
            source_version: provenance.source_version,
            current_source_version: None,
            source_deleted: true,
        }),
        Err(_) => None,
    }
}

/// GET /v1/memories/{id}
pub async fn get_memory(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Path(id): Path<String>,
    Query(params): Query<GetMemoryQuery>,
) -> Result<Json<serde_json::Value>, OmemError> {
    let store = state.store_manager.get_store(&personal_space_id(&auth.tenant_id)).await?;
    let memory = store
        .get_by_id(&id)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("memory {id}")))?;

    let mut response = serde_json::to_value(&memory)
        .map_err(|e| OmemError::Internal(format!("serialize failed: {e}")))?;

    if params.check_stale {
        if let Some(stale_info) = check_stale_for_memory(&memory, &state.store_manager).await {
            response["stale_info"] = serde_json::to_value(&stale_info)
                .map_err(|e| OmemError::Internal(format!("serialize stale_info: {e}")))?;
        }
    }

    Ok(Json(response))
}

/// PUT /v1/memories/{id}
pub async fn update_memory(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Path(id): Path<String>,
    Json(body): Json<UpdateMemoryBody>,
) -> Result<Json<Memory>, OmemError> {
    let store = state.store_manager.get_store(&personal_space_id(&auth.tenant_id)).await?;
    let mut memory = store
        .get_by_id(&id)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("memory {id}")))?;

    let mut need_reembed = false;

    if let Some(content) = body.content {
        if content.is_empty() {
            return Err(OmemError::Validation("content cannot be empty".to_string()));
        }
        memory.content = content.clone();
        memory.l2_content = content;
        need_reembed = true;
    }

    if let Some(tags) = body.tags {
        memory.tags = tags;
    }

    if let Some(state_str) = body.state {
        memory.state = state_str
            .parse()
            .map_err(|e: String| OmemError::Validation(e))?;
    }

    memory.updated_at = chrono::Utc::now().to_rfc3339();

    let vector = if need_reembed {
        let vectors = state
            .embed
            .embed(&[memory.content.clone()])
            .await
            .map_err(|e| OmemError::Embedding(format!("failed to embed content: {e}")))?;
        vectors.into_iter().next()
    } else {
        None
    };

    store.update(&memory, vector.as_deref()).await?;

    Ok(Json(memory))
}

/// DELETE /v1/memories/{id}
pub async fn delete_memory(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, OmemError> {
    let store = state.store_manager.get_store(&personal_space_id(&auth.tenant_id)).await?;
    store
        .get_by_id(&id)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("memory {id}")))?;

    store.soft_delete(&id).await?;

    Ok(Json(serde_json::json!({"status": "deleted"})))
}

/// GET /v1/memories
pub async fn list_memories(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Query(params): Query<ListQuery>,
) -> Result<Json<ListResponseDto>, OmemError> {
    let store = state.store_manager.get_store(&personal_space_id(&auth.tenant_id)).await?;

    let filter = ListFilter {
        category: params.category,
        tier: params.tier,
        tags: params
            .tags
            .map(|t| t.split(',').map(|s| s.trim().to_string()).collect()),
        memory_type: params.memory_type,
        state: params.state,
        sort: params.sort,
        order: params.order,
    };

    let total_count = store.count_filtered(&filter).await?;
    let memories = store
        .list_filtered(&filter, params.limit, params.offset)
        .await?;

    Ok(Json(ListResponseDto {
        memories,
        total_count,
        limit: params.limit,
        offset: params.offset,
    }))
}

// ── Batch Delete ─────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct BatchDeleteRequest {
    pub memory_ids: Option<Vec<String>>,
    pub filter: Option<BatchDeleteFilter>,
    #[serde(default)]
    pub confirm: bool,
}

#[derive(Deserialize)]
pub struct BatchDeleteFilter {
    pub source: Option<String>,
    pub tags: Option<Vec<String>>,
    pub category: Option<String>,
    pub memory_type: Option<String>,
    pub state: Option<String>,
    pub before: Option<String>,
}

fn build_batch_delete_where(filter: &BatchDeleteFilter) -> String {
    let mut conditions = Vec::new();

    if let Some(ref source) = filter.source {
        conditions.push(format!(
            "source LIKE '{}%'",
            source.replace('\'', "''")
        ));
    }
    if let Some(ref tags) = filter.tags {
        for tag in tags {
            let escaped = tag.replace('\'', "''");
            conditions.push(format!("(tags LIKE '%\"{}\"%')", escaped));
        }
    }
    if let Some(ref cat) = filter.category {
        conditions.push(format!("category = '{}'", cat.replace('\'', "''")));
    }
    if let Some(ref mt) = filter.memory_type {
        conditions.push(format!("memory_type = '{}'", mt.replace('\'', "''")));
    }
    if let Some(ref state) = filter.state {
        conditions.push(format!("state = '{}'", state.replace('\'', "''")));
    }
    if let Some(ref before) = filter.before {
        conditions.push(format!("created_at < '{}'", before.replace('\'', "''")));
    }

    if conditions.is_empty() {
        "true".to_string()
    } else {
        conditions.join(" AND ")
    }
}

pub async fn batch_delete(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Json(body): Json<BatchDeleteRequest>,
) -> Result<Json<serde_json::Value>, OmemError> {
    let store = state
        .store_manager
        .get_store(&personal_space_id(&auth.tenant_id))
        .await?;

    if let Some(ids) = body.memory_ids {
        let mut deleted = 0usize;
        for id in &ids {
            if store.get_by_id(id).await?.is_some() {
                store.soft_delete(id).await?;
                deleted += 1;
            }
        }
        return Ok(Json(serde_json::json!({
            "deleted": deleted,
            "mode": "ids"
        })));
    }

    if let Some(ref filter) = body.filter {
        let where_clause = build_batch_delete_where(filter);

        if !body.confirm {
            let count = store.count_by_filter(&where_clause).await?;
            return Ok(Json(serde_json::json!({
                "would_delete": count
            })));
        }

        let deleted = store.batch_soft_delete(&where_clause).await?;
        return Ok(Json(serde_json::json!({
            "deleted": deleted,
            "mode": "filter"
        })));
    }

    Err(OmemError::Validation(
        "provide either memory_ids or filter".to_string(),
    ))
}

pub async fn delete_all_memories(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    headers: HeaderMap,
) -> Result<Json<serde_json::Value>, OmemError> {
    let confirm = headers.get("X-Confirm").and_then(|v| v.to_str().ok());
    if confirm != Some("delete-all") {
        return Err(OmemError::Validation(
            "DELETE /v1/memories/all requires X-Confirm: delete-all header".to_string(),
        ));
    }

    let store = state
        .store_manager
        .get_store(&personal_space_id(&auth.tenant_id))
        .await?;
    let count = store.delete_all().await?;

    let session_uri = format!("{}/{}", state.config.store_uri(), personal_space_id(&auth.tenant_id));
    let session_store = SessionStore::new(&session_uri)
        .await
        .map_err(|e| OmemError::Storage(format!("session store: {e}")))?;
    session_store.init_table().await?;
    let sessions_cleared = session_store.delete_all().await?;

    Ok(Json(serde_json::json!({
        "deleted": count,
        "sessions_cleared": sessions_cleared
    })))
}
