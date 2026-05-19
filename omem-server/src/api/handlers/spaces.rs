use std::sync::Arc;

use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Deserialize;

use crate::api::server::AppState;
use crate::domain::error::OmemError;
use crate::domain::space::{MemberRole, Space, SpaceMember, SpaceType};
use crate::domain::tenant::AuthInfo;

#[derive(Deserialize)]
pub struct CreateSpaceRequest {
    pub name: String,
    pub space_type: String,
    pub members: Option<Vec<CreateMemberRequest>>,
}

#[derive(Deserialize)]
pub struct CreateMemberRequest {
    pub user_id: String,
    pub role: String,
}

#[derive(Deserialize)]
pub struct UpdateSpaceRequest {
    pub name: Option<String>,
}

#[derive(Deserialize)]
pub struct UpdateMemberRoleRequest {
    pub role: String,
}

fn parse_space_type(s: &str) -> Result<SpaceType, OmemError> {
    match s {
        "personal" => Ok(SpaceType::Personal),
        "team" => Ok(SpaceType::Team),
        "organization" => Ok(SpaceType::Organization),
        _ => Err(OmemError::Validation(format!(
            "invalid space_type '{}': must be personal, team, or organization",
            s
        ))),
    }
}

fn parse_member_role(s: &str) -> Result<MemberRole, OmemError> {
    match s {
        "admin" => Ok(MemberRole::Admin),
        "member" => Ok(MemberRole::Member),
        "reader" => Ok(MemberRole::Reader),
        _ => Err(OmemError::Validation(format!(
            "invalid role '{}': must be admin, member, or reader",
            s
        ))),
    }
}

pub async fn create_space(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Json(body): Json<CreateSpaceRequest>,
) -> Result<impl IntoResponse, OmemError> {
    if body.name.is_empty() {
        return Err(OmemError::Validation("name is required".to_string()));
    }

    let space_type = parse_space_type(&body.space_type)?;
    let now = chrono::Utc::now().to_rfc3339();
    let prefix = match space_type {
        SpaceType::Personal => "personal",
        SpaceType::Team => "team",
        SpaceType::Organization => "org",
    };
    let id = format!("{}/{}", prefix, uuid::Uuid::new_v4());

    let mut members = vec![SpaceMember {
        user_id: auth.tenant_id.clone(),
        role: MemberRole::Admin,
        joined_at: now.clone(),
    }];

    if let Some(extra) = body.members {
        for m in extra {
            let role = parse_member_role(&m.role)?;
            if m.user_id == auth.tenant_id {
                continue;
            }
            members.push(SpaceMember {
                user_id: m.user_id,
                role,
                joined_at: now.clone(),
            });
        }
    }

    let space = Space {
        id,
        space_type,
        name: body.name,
        owner_id: auth.tenant_id,
        members,
        auto_share_rules: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
    };

    state.space_store.create_space(&space).await?;

    Ok((StatusCode::CREATED, Json(space)))
}

pub async fn list_spaces(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
) -> Result<Json<Vec<Space>>, OmemError> {
    let spaces = state
        .space_store
        .list_spaces_for_user(&auth.tenant_id)
        .await?;
    Ok(Json(spaces))
}

pub async fn get_space(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Path(id): Path<String>,
) -> Result<Json<Space>, OmemError> {
    let space = state
        .space_store
        .get_space(&id)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("space {id}")))?;

    if space.owner_id != auth.tenant_id
        && !space.members.iter().any(|m| m.user_id == auth.tenant_id)
    {
        return Err(OmemError::Unauthorized(
            "not a member of this space".to_string(),
        ));
    }

    Ok(Json(space))
}

pub async fn update_space(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Path(id): Path<String>,
    Json(body): Json<UpdateSpaceRequest>,
) -> Result<Json<Space>, OmemError> {
    let mut space = state
        .space_store
        .get_space(&id)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("space {id}")))?;

    let is_admin = space.owner_id == auth.tenant_id
        || space
            .members
            .iter()
            .any(|m| m.user_id == auth.tenant_id && m.role == MemberRole::Admin);

    if !is_admin {
        return Err(OmemError::Unauthorized("admin access required".to_string()));
    }

    if let Some(name) = body.name {
        if name.is_empty() {
            return Err(OmemError::Validation("name cannot be empty".to_string()));
        }
        space.name = name;
    }

    space.updated_at = chrono::Utc::now().to_rfc3339();
    state.space_store.update_space(&space).await?;

    Ok(Json(space))
}

pub async fn delete_space(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, OmemError> {
    let space = state
        .space_store
        .get_space(&id)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("space {id}")))?;

    let is_admin = space.owner_id == auth.tenant_id
        || space
            .members
            .iter()
            .any(|m| m.user_id == auth.tenant_id && m.role == MemberRole::Admin);

    if !is_admin {
        return Err(OmemError::Unauthorized(
            "admin access required to delete space".to_string(),
        ));
    }

    state.space_store.delete_space(&id).await?;

    Ok(Json(serde_json::json!({"status": "deleted"})))
}

pub async fn add_member(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Path(space_id): Path<String>,
    Json(body): Json<CreateMemberRequest>,
) -> Result<Json<Space>, OmemError> {
    let mut space = state
        .space_store
        .get_space(&space_id)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("space {space_id}")))?;

    let is_admin = space.owner_id == auth.tenant_id
        || space
            .members
            .iter()
            .any(|m| m.user_id == auth.tenant_id && m.role == MemberRole::Admin);

    if !is_admin {
        return Err(OmemError::Unauthorized(
            "admin access required to add members".to_string(),
        ));
    }

    if space.members.iter().any(|m| m.user_id == body.user_id) {
        return Err(OmemError::Validation(format!(
            "user {} is already a member",
            body.user_id
        )));
    }

    let role = parse_member_role(&body.role)?;
    space.members.push(SpaceMember {
        user_id: body.user_id,
        role,
        joined_at: chrono::Utc::now().to_rfc3339(),
    });
    space.updated_at = chrono::Utc::now().to_rfc3339();

    state.space_store.update_space(&space).await?;

    Ok(Json(space))
}

#[derive(Deserialize)]
pub struct MemberPath {
    pub id: String,
    pub user_id: String,
}

pub async fn remove_member(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Path(path): Path<MemberPath>,
) -> Result<Json<Space>, OmemError> {
    let mut space = state
        .space_store
        .get_space(&path.id)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("space {}", path.id)))?;

    let is_admin = space.owner_id == auth.tenant_id
        || space
            .members
            .iter()
            .any(|m| m.user_id == auth.tenant_id && m.role == MemberRole::Admin);

    if !is_admin {
        return Err(OmemError::Unauthorized(
            "admin access required to remove members".to_string(),
        ));
    }

    if path.user_id == space.owner_id {
        return Err(OmemError::Validation(
            "cannot remove the space owner".to_string(),
        ));
    }

    let before = space.members.len();
    space.members.retain(|m| m.user_id != path.user_id);
    if space.members.len() == before {
        return Err(OmemError::NotFound(format!(
            "member {} in space {}",
            path.user_id, path.id
        )));
    }

    space.updated_at = chrono::Utc::now().to_rfc3339();
    state.space_store.update_space(&space).await?;

    Ok(Json(space))
}

pub async fn update_member_role(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Path(path): Path<MemberPath>,
    Json(body): Json<UpdateMemberRoleRequest>,
) -> Result<Json<Space>, OmemError> {
    let mut space = state
        .space_store
        .get_space(&path.id)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("space {}", path.id)))?;

    let is_admin = space.owner_id == auth.tenant_id
        || space
            .members
            .iter()
            .any(|m| m.user_id == auth.tenant_id && m.role == MemberRole::Admin);

    if !is_admin {
        return Err(OmemError::Unauthorized(
            "admin access required to update member roles".to_string(),
        ));
    }

    let new_role = parse_member_role(&body.role)?;
    let member = space
        .members
        .iter_mut()
        .find(|m| m.user_id == path.user_id)
        .ok_or_else(|| {
            OmemError::NotFound(format!("member {} in space {}", path.user_id, path.id))
        })?;

    member.role = new_role;
    space.updated_at = chrono::Utc::now().to_rfc3339();
    state.space_store.update_space(&space).await?;

    Ok(Json(space))
}
