use std::sync::Arc;

use axum::extract::{Extension, Path, Query, State};
use axum::http::StatusCode;
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
}

#[derive(Serialize)]
pub struct SearchResponseDto {
    pub results: Vec<SearchResultDto>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trace: Option<serde_json::Value>,
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
            SessionStore::new(&state.config.store_uri())
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
        };

        let retrieval_pipeline = RetrievalPipeline::new(store);
        let search_results = retrieval_pipeline.search(&request).await?;

        let results: Vec<SearchResultDto> = search_results
            .results
            .into_iter()
            .map(|r| SearchResultDto {
                memory: r.memory,
                score: r.score,
            })
            .collect();

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

    let mut all_results: Vec<(Memory, f32, String)> = Vec::new();

    for acc in &accessible {
        let request = SearchRequest {
            query: params.q.clone(),
            query_vector: query_vector.clone(),
            tenant_id: auth.tenant_id.clone(),
            scope_filter: params.scope.clone(),
            limit: Some(params.limit),
            min_score: params.min_score,
            include_trace: false,
        };

        let pipeline = RetrievalPipeline::new(acc.store.clone());
        match pipeline.search(&request).await {
            Ok(search_results) => {
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
                    let weighted = normalized * acc.weight;
                    all_results.push((r.memory, weighted, acc.space_id.clone()));
                }
            }
            Err(e) => {
                tracing::warn!(space_id = %acc.space_id, error = %e, "space search failed, skipping");
            }
        }
    }

    all_results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    all_results.truncate(params.limit);

    let results: Vec<SearchResultDto> = all_results
        .into_iter()
        .map(|(memory, score, _space_id)| SearchResultDto { memory, score })
        .collect();

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

/// GET /v1/memories/{id}
pub async fn get_memory(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Path(id): Path<String>,
) -> Result<Json<Memory>, OmemError> {
    let store = state.store_manager.get_store(&personal_space_id(&auth.tenant_id)).await?;
    let memory = store
        .get_by_id(&id)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("memory {id}")))?;

    Ok(Json(memory))
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
