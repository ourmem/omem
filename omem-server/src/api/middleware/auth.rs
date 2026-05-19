use std::sync::Arc;

use axum::extract::{Request, State};
use axum::middleware::Next;
use axum::response::Response;

use crate::api::server::AppState;
use crate::domain::error::OmemError;
use crate::domain::tenant::AuthInfo;

/// Middleware that validates the X-API-Key header and resolves the tenant.
///
/// On success, injects `AuthInfo` into request extensions.
/// On failure, returns 401 Unauthorized.
pub async fn auth_middleware(
    State(state): State<Arc<AppState>>,
    mut request: Request,
    next: Next,
) -> Result<Response, OmemError> {
    let api_key = request
        .headers()
        .get("x-api-key")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let api_key =
        api_key.ok_or_else(|| OmemError::Unauthorized("missing X-API-Key header".to_string()))?;

    let tenant = state
        .tenant_store
        .get_by_id(&api_key)
        .await
        .map_err(|e| OmemError::Internal(format!("tenant lookup failed: {e}")))?
        .ok_or_else(|| OmemError::Unauthorized("invalid API key".to_string()))?;

    if tenant.status.to_string() != "active" {
        return Err(OmemError::Unauthorized(format!(
            "tenant {} is {}",
            tenant.id, tenant.status
        )));
    }

    let agent_id = request
        .headers()
        .get("x-agent-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());

    let auth_info = AuthInfo {
        tenant_id: tenant.id,
        agent_id,
    };

    request.extensions_mut().insert(auth_info);

    Ok(next.run(request).await)
}
