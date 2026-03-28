use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;
use uuid::Uuid;

use crate::api::server::AppState;
use crate::domain::error::OmemError;
use crate::domain::space::{MemberRole, Space, SpaceMember, SpaceType};
use crate::domain::tenant::{Tenant, TenantConfig, TenantStatus};

#[derive(Deserialize)]
pub struct CreateTenantBody {
    pub name: String,
}

/// POST /v1/tenants — No auth required.
/// Creates a new tenant and returns the id as the API key.
/// Also auto-creates a personal space for the tenant.
pub async fn create_tenant(
    State(state): State<Arc<AppState>>,
    Json(body): Json<CreateTenantBody>,
) -> Result<impl IntoResponse, OmemError> {
    if body.name.is_empty() {
        return Err(OmemError::Validation("name is required".to_string()));
    }

    let id = Uuid::new_v4().to_string();
    let tenant = Tenant {
        id: id.clone(),
        name: body.name.clone(),
        status: TenantStatus::Active,
        config: TenantConfig::default(),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    state.tenant_store.create(&tenant).await?;

    let now = chrono::Utc::now().to_rfc3339();
    let personal_space = Space {
        id: format!("personal/{id}"),
        space_type: SpaceType::Personal,
        name: body.name,
        owner_id: id.clone(),
        members: vec![SpaceMember {
            user_id: id.clone(),
            role: MemberRole::Admin,
            joined_at: now.clone(),
        }],
        auto_share_rules: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
    };

    state.space_store.create_space(&personal_space).await?;

    Ok((
        StatusCode::OK,
        Json(serde_json::json!({
            "id": id,
            "api_key": id,
            "status": "active",
        })),
    ))
}
