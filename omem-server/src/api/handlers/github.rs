use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Extension, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use crate::api::server::{AppState, personal_space_id};
use crate::connectors::github::{ConnectRequest, GitHubConnector, WebhookPayload};
use crate::domain::error::OmemError;
use crate::domain::tenant::AuthInfo;

#[derive(Serialize)]
pub struct WebhookResponse {
    pub event_type: String,
    pub memories_created: usize,
}

pub async fn github_webhook(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<impl IntoResponse, OmemError> {
    let webhook_secret = std::env::var("OMEM_GITHUB_WEBHOOK_SECRET").ok();

    let tenant_id = headers
        .get("x-tenant-id")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("default");

    let store = state.store_manager.get_store(tenant_id).await?;

    let connector = GitHubConnector::new(
        store,
        state.embed.clone(),
        webhook_secret,
    );

    if connector.webhook_secret.is_some() {
        let signature = headers
            .get("x-hub-signature-256")
            .and_then(|v| v.to_str().ok())
            .ok_or_else(|| {
                OmemError::Unauthorized("missing X-Hub-Signature-256 header".to_string())
            })?;
        connector.verify_signature(&body, signature)?;
    }

    let event_type = headers
        .get("x-github-event")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("unknown");

    let payload: WebhookPayload = serde_json::from_slice(&body)
        .map_err(|e| OmemError::Validation(format!("invalid webhook payload: {e}")))?;

    let memories = connector.process_webhook(event_type, &payload, tenant_id)?;
    let count = connector.store_memories(memories).await?;

    Ok((
        StatusCode::OK,
        Json(WebhookResponse {
            event_type: event_type.to_string(),
            memories_created: count,
        }),
    ))
}

pub async fn github_connect(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Json(body): Json<ConnectRequest>,
) -> Result<impl IntoResponse, OmemError> {
    if body.repo.is_empty() {
        return Err(OmemError::Validation("repo is required".to_string()));
    }
    if body.access_token.is_empty() {
        return Err(OmemError::Validation("access_token is required".to_string()));
    }

    let store = state.store_manager.get_store(&personal_space_id(&auth.tenant_id)).await?;
    let webhook_secret = std::env::var("OMEM_GITHUB_WEBHOOK_SECRET").ok();

    let connector = GitHubConnector::new(
        store,
        state.embed.clone(),
        webhook_secret,
    );

    let response = connector
        .register_webhook(&body.access_token, &body.repo, &body.webhook_url)
        .await?;

    Ok((StatusCode::OK, Json(serde_json::json!(response))))
}
