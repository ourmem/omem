use std::sync::Arc;

use axum::extract::{Extension, Query, State};
use axum::Json;
use serde::Deserialize;

use crate::api::server::{AppState, personal_space_id};
use crate::domain::error::OmemError;
use crate::domain::tenant::AuthInfo;
use crate::profile::service::ProfileService;

#[derive(Deserialize)]
pub struct ProfileQuery {
    #[serde(default)]
    pub q: String,
}

/// GET /v1/profile — Returns aggregated user profile from ProfileService.
pub async fn get_profile(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Query(params): Query<ProfileQuery>,
) -> Result<Json<serde_json::Value>, OmemError> {
    let store = state.store_manager.get_store(&personal_space_id(&auth.tenant_id)).await?;
    let profile_service = ProfileService::new(store);

    let query = if params.q.is_empty() {
        None
    } else {
        Some(params.q.as_str())
    };
    let response = profile_service.get_profile(query).await?;

    Ok(Json(serde_json::json!({
        "static_facts": response.profile.static_facts,
        "dynamic_context": response.profile.dynamic_context,
        "search_results": response.search_results.map(|results| {
            results.iter().map(|r| serde_json::json!({
                "score": r.score,
                "memory": {
                    "id": r.memory.id,
                    "content": r.memory.content,
                    "category": r.memory.category.to_string(),
                    "tags": r.memory.tags,
                }
            })).collect::<Vec<_>>()
        })
    })))
}
