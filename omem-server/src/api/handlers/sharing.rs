use std::sync::Arc;

use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::server::{normalize_space_id, personal_space_id, AppState};
use crate::domain::error::OmemError;
use crate::domain::memory::Memory;
use crate::domain::space::{
    AutoShareRule, MemberRole, Provenance, SharingAction, SharingEvent, Space, SpaceMember,
    SpaceType,
};
use crate::domain::tenant::AuthInfo;
use crate::store::StoreManager;

// ── Request DTOs ─────────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ShareRequest {
    pub target_space: String,
    pub note: Option<String>,
}

#[derive(Deserialize)]
pub struct PullRequest {
    pub source_space: String,
    pub visibility: Option<String>,
}

#[derive(Deserialize)]
pub struct UnshareRequest {
    pub target_space: String,
}

#[derive(Deserialize)]
pub struct BatchShareRequest {
    pub memory_ids: Vec<String>,
    pub target_space: String,
}

#[derive(Serialize)]
pub struct BatchShareResult {
    pub succeeded: Vec<Memory>,
    pub failed: Vec<BatchShareError>,
}

#[derive(Serialize)]
pub struct BatchShareError {
    pub memory_id: String,
    pub error: String,
}

#[derive(Deserialize)]
pub struct CreateAutoShareRuleRequest {
    pub source_space: String,
    pub categories: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub min_importance: Option<f32>,
    pub require_approval: Option<bool>,
}

#[derive(Deserialize)]
pub struct ReshareRequest {
    pub target_space: Option<String>,
}

#[derive(Deserialize)]
pub struct AutoShareRulePath {
    pub id: String,
    pub rule_id: String,
}

// ── share-all DTOs ──────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ShareAllFilters {
    pub categories: Option<Vec<String>>,
    pub tags: Option<Vec<String>>,
    pub min_importance: Option<f32>,
}

#[derive(Deserialize)]
pub struct ShareAllRequest {
    pub target_space: String,
    pub filters: Option<ShareAllFilters>,
}

#[derive(Serialize)]
pub struct ShareAllResponse {
    pub total: usize,
    pub shared: usize,
    pub skipped_existing: usize,
    pub failed: usize,
}

// ── share-to-user DTOs ──────────────────────────────────────────────

#[derive(Deserialize)]
pub struct ShareToUserRequest {
    pub target_user: String,
    pub note: Option<String>,
}

#[derive(Serialize)]
pub struct ShareToUserResponse {
    pub space_id: String,
    pub shared_copy_id: String,
    pub space_created: bool,
}

// ── share-all-to-user DTOs ──────────────────────────────────────────

#[derive(Deserialize)]
pub struct ShareAllToUserRequest {
    pub target_user: String,
    pub filters: Option<ShareAllFilters>,
}

#[derive(Serialize)]
pub struct ShareAllToUserResponse {
    pub space_id: String,
    pub space_created: bool,
    pub total: usize,
    pub shared: usize,
    pub skipped_existing: usize,
    pub failed: usize,
}

// ── org/setup DTOs ──────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct OrgSetupMember {
    pub user_id: String,
    pub role: String,
}

#[derive(Deserialize)]
pub struct OrgSetupRequest {
    pub name: String,
    pub members: Vec<OrgSetupMember>,
}

#[derive(Serialize)]
pub struct OrgSetupResponse {
    pub space_id: String,
    pub name: String,
    pub members_added: usize,
    pub failed_members: Vec<String>,
}

// ── org/publish DTOs ────────────────────────────────────────────────

#[derive(Deserialize)]
pub struct OrgPublishAutoShareRule {
    pub categories: Option<Vec<String>>,
    pub min_importance: Option<f32>,
}

#[derive(Deserialize)]
pub struct OrgPublishRequest {
    pub memory_ids: Option<Vec<String>>,
    pub auto_share_rule: Option<OrgPublishAutoShareRule>,
}

#[derive(Serialize)]
pub struct OrgPublishResponse {
    pub published: usize,
    pub skipped_existing: usize,
    pub failed: usize,
    pub auto_share_rule_id: Option<String>,
}

// ── Helpers ──────────────────────────────────────────────────────────

fn verify_space_access(space: &Space, user_id: &str) -> Result<(), OmemError> {
    if space.owner_id == user_id {
        return Ok(());
    }
    if space.members.iter().any(|m| m.user_id == user_id) {
        return Ok(());
    }
    Err(OmemError::Unauthorized(format!(
        "no access to space {}",
        space.id
    )))
}

fn verify_space_write_access(space: &Space, user_id: &str) -> Result<(), OmemError> {
    if space.owner_id == user_id {
        return Ok(());
    }
    for m in &space.members {
        if m.user_id == user_id {
            return match m.role {
                MemberRole::Admin | MemberRole::Member => Ok(()),
                MemberRole::Reader => Err(OmemError::Unauthorized(
                    "read-only access to target space".to_string(),
                )),
            };
        }
    }
    Err(OmemError::Unauthorized(format!(
        "no access to space {}",
        space.id
    )))
}

fn make_shared_copy(source: &Memory, target_space: &str, user_id: &str, agent_id: &str) -> Memory {
    let now = chrono::Utc::now().to_rfc3339();
    Memory {
        id: Uuid::new_v4().to_string(),
        content: source.content.clone(),
        l0_abstract: source.l0_abstract.clone(),
        l1_overview: source.l1_overview.clone(),
        l2_content: source.l2_content.clone(),
        category: source.category.clone(),
        memory_type: source.memory_type.clone(),
        state: source.state.clone(),
        tier: source.tier.clone(),
        importance: source.importance,
        confidence: source.confidence,
        access_count: 0,
        tags: source.tags.clone(),
        scope: source.scope.clone(),
        agent_id: source.agent_id.clone(),
        session_id: source.session_id.clone(),
        tenant_id: source.tenant_id.clone(),
        source: source.source.clone(),
        relations: source.relations.clone(),
        superseded_by: None,
        invalidated_at: None,
        created_at: now.clone(),
        updated_at: now.clone(),
        last_accessed_at: None,
        space_id: target_space.to_string(),
        visibility: "global".to_string(),
        owner_agent_id: source.owner_agent_id.clone(),
        provenance: Some(Provenance {
            shared_from_space: source.space_id.clone(),
            shared_from_memory: source.id.clone(),
            shared_by_user: user_id.to_string(),
            shared_by_agent: agent_id.to_string(),
            shared_at: now,
            original_created_at: source.created_at.clone(),
            source_version: source.version,
        }),
        version: Some(1),
    }
}

fn make_sharing_event(
    action: SharingAction,
    memory_id: &str,
    from_space: &str,
    to_space: &str,
    user_id: &str,
    agent_id: &str,
    content_preview: &str,
) -> SharingEvent {
    let preview = if content_preview.len() > 100 {
        format!("{}...", &content_preview[..97])
    } else {
        content_preview.to_string()
    };
    SharingEvent {
        id: Uuid::new_v4().to_string(),
        action,
        memory_id: memory_id.to_string(),
        from_space: from_space.to_string(),
        to_space: to_space.to_string(),
        user_id: user_id.to_string(),
        agent_id: agent_id.to_string(),
        content_preview: preview,
        timestamp: chrono::Utc::now().to_rfc3339(),
    }
}

fn content_preview(content: &str) -> String {
    if content.len() > 100 {
        format!("{}...", &content[..97])
    } else {
        content.to_string()
    }
}

// ── Handlers ─────────────────────────────────────────────────────────

pub async fn share_memory(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Path(id): Path<String>,
    Json(body): Json<ShareRequest>,
) -> Result<impl IntoResponse, OmemError> {
    if body.target_space.is_empty() {
        return Err(OmemError::Validation(
            "target_space is required".to_string(),
        ));
    }

    let source_store = state
        .store_manager
        .get_store(&personal_space_id(&auth.tenant_id))
        .await?;
    let source_memory = source_store
        .get_by_id(&id)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("memory {id}")))?;

    let target_space = state
        .space_store
        .get_space(&body.target_space)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("space {}", body.target_space)))?;

    verify_space_write_access(&target_space, &auth.tenant_id)?;

    let target_store = state.store_manager.get_store(&target_space.id).await?;

    // Circular sharing prevention: return existing copy if already shared
    let existing = target_store
        .find_by_provenance_source(&source_memory.id)
        .await?;
    if let Some(copy) = existing.into_iter().next() {
        return Ok((StatusCode::OK, Json(copy)));
    }

    let agent_id = auth.agent_id.as_deref().unwrap_or("");
    let source_vector = source_store.get_vector_by_id(&source_memory.id).await?;
    let copy = make_shared_copy(&source_memory, &target_space.id, &auth.tenant_id, agent_id);
    target_store.create(&copy, source_vector.as_deref()).await?;

    let event = make_sharing_event(
        SharingAction::Share,
        &copy.id,
        &source_memory.space_id,
        &target_space.id,
        &auth.tenant_id,
        agent_id,
        &content_preview(&source_memory.content),
    );
    state.space_store.record_sharing_event(&event).await?;

    Ok((StatusCode::CREATED, Json(copy)))
}

pub async fn pull_memory(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Path(id): Path<String>,
    Json(body): Json<PullRequest>,
) -> Result<impl IntoResponse, OmemError> {
    if body.source_space.is_empty() {
        return Err(OmemError::Validation(
            "source_space is required".to_string(),
        ));
    }

    let source_space = state
        .space_store
        .get_space(&body.source_space)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("space {}", body.source_space)))?;

    verify_space_access(&source_space, &auth.tenant_id)?;

    let source_store = state.store_manager.get_store(&source_space.id).await?;
    let source_memory = source_store.get_by_id(&id).await?.ok_or_else(|| {
        OmemError::NotFound(format!("memory {id} in space {}", body.source_space))
    })?;
    let source_vector = source_store.get_vector_by_id(&source_memory.id).await?;

    let personal_store = state
        .store_manager
        .get_store(&personal_space_id(&auth.tenant_id))
        .await?;
    let visibility = body.visibility.unwrap_or_else(|| "private".to_string());
    let agent_id = auth.agent_id.as_deref().unwrap_or("");

    let now = chrono::Utc::now().to_rfc3339();
    let mut copy = make_shared_copy(&source_memory, &auth.tenant_id, &auth.tenant_id, agent_id);
    copy.visibility = visibility;
    copy.provenance = Some(Provenance {
        shared_from_space: source_space.id.clone(),
        shared_from_memory: source_memory.id.clone(),
        shared_by_user: auth.tenant_id.clone(),
        shared_by_agent: agent_id.to_string(),
        shared_at: now,
        original_created_at: source_memory.created_at.clone(),
        source_version: source_memory.version,
    });

    personal_store
        .create(&copy, source_vector.as_deref())
        .await?;

    let event = make_sharing_event(
        SharingAction::Pull,
        &copy.id,
        &source_space.id,
        &auth.tenant_id,
        &auth.tenant_id,
        agent_id,
        &content_preview(&source_memory.content),
    );
    state.space_store.record_sharing_event(&event).await?;

    Ok((StatusCode::CREATED, Json(copy)))
}

pub async fn unshare_memory(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Path(id): Path<String>,
    Json(body): Json<UnshareRequest>,
) -> Result<Json<serde_json::Value>, OmemError> {
    if body.target_space.is_empty() {
        return Err(OmemError::Validation(
            "target_space is required".to_string(),
        ));
    }

    let target_space = state
        .space_store
        .get_space(&body.target_space)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("space {}", body.target_space)))?;

    verify_space_write_access(&target_space, &auth.tenant_id)?;

    let target_store = state.store_manager.get_store(&target_space.id).await?;
    let copies = target_store.find_by_provenance_source(&id).await?;

    if copies.is_empty() {
        return Err(OmemError::NotFound(format!(
            "no shared copy of memory {id} in space {}",
            body.target_space
        )));
    }

    let copy = &copies[0];
    if let Some(ref prov) = copy.provenance {
        if prov.shared_by_user != auth.tenant_id {
            let is_admin = target_space.owner_id == auth.tenant_id
                || target_space
                    .members
                    .iter()
                    .any(|m| m.user_id == auth.tenant_id && m.role == MemberRole::Admin);
            if !is_admin {
                return Err(OmemError::Unauthorized(
                    "only the sharer or admin can unshare".to_string(),
                ));
            }
        }
    }

    target_store.soft_delete(&copy.id).await?;

    let agent_id = auth.agent_id.as_deref().unwrap_or("");
    let event = make_sharing_event(
        SharingAction::Unshare,
        &id,
        &copy.space_id,
        &auth.tenant_id,
        &auth.tenant_id,
        agent_id,
        &content_preview(&copy.content),
    );
    state.space_store.record_sharing_event(&event).await?;

    Ok(Json(serde_json::json!({"status": "unshared"})))
}

pub async fn batch_share(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Json(body): Json<BatchShareRequest>,
) -> Result<impl IntoResponse, OmemError> {
    if body.memory_ids.is_empty() {
        return Err(OmemError::Validation(
            "memory_ids cannot be empty".to_string(),
        ));
    }
    if body.memory_ids.len() > 500 {
        return Err(OmemError::Validation(
            "batch_share limited to 500 memories".to_string(),
        ));
    }
    if body.target_space.is_empty() {
        return Err(OmemError::Validation(
            "target_space is required".to_string(),
        ));
    }

    let target_space = state
        .space_store
        .get_space(&body.target_space)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("space {}", body.target_space)))?;

    verify_space_write_access(&target_space, &auth.tenant_id)?;

    let source_store = state
        .store_manager
        .get_store(&personal_space_id(&auth.tenant_id))
        .await?;
    let target_store = state.store_manager.get_store(&target_space.id).await?;
    let agent_id = auth.agent_id.as_deref().unwrap_or("").to_string();

    use futures::stream::{self, StreamExt};

    let results: Vec<(String, Result<Memory, OmemError>)> =
        stream::iter(body.memory_ids.into_iter())
            .map(|mem_id| {
                let source_store = source_store.clone();
                let target_store = target_store.clone();
                let space_store = state.space_store.clone();
                let target_space_id = target_space.id.clone();
                let user_id = auth.tenant_id.clone();
                let agent_id = agent_id.clone();
                async move {
                    let result = share_single(
                        &source_store,
                        &target_store,
                        &space_store,
                        &mem_id,
                        &target_space_id,
                        &user_id,
                        &agent_id,
                    )
                    .await;
                    (mem_id, result)
                }
            })
            .buffer_unordered(10)
            .collect()
            .await;

    let mut succeeded = Vec::new();
    let mut failed = Vec::new();
    for (mem_id, result) in results {
        match result {
            Ok(copy) => succeeded.push(copy),
            Err(e) => failed.push(BatchShareError {
                memory_id: mem_id,
                error: e.to_string(),
            }),
        }
    }

    let result = BatchShareResult { succeeded, failed };
    Ok((StatusCode::OK, Json(result)))
}

async fn share_single(
    source_store: &crate::store::LanceStore,
    target_store: &crate::store::LanceStore,
    space_store: &crate::store::SpaceStore,
    memory_id: &str,
    target_space_id: &str,
    user_id: &str,
    agent_id: &str,
) -> Result<Memory, OmemError> {
    let source = source_store
        .get_by_id(memory_id)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("memory {memory_id}")))?;

    let existing = target_store.find_by_provenance_source(&source.id).await?;
    if let Some(copy) = existing.into_iter().next() {
        return Ok(copy);
    }

    let source_vector = source_store.get_vector_by_id(&source.id).await?;
    let copy = make_shared_copy(&source, target_space_id, user_id, agent_id);
    target_store.create(&copy, source_vector.as_deref()).await?;

    let event = make_sharing_event(
        SharingAction::BatchShare,
        &copy.id,
        &source.space_id,
        target_space_id,
        user_id,
        agent_id,
        &content_preview(&source.content),
    );
    space_store.record_sharing_event(&event).await?;

    Ok(copy)
}

// ── Auto-share rule handlers ─────────────────────────────────────────

pub async fn create_auto_share_rule(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Path(space_id): Path<String>,
    Json(body): Json<CreateAutoShareRuleRequest>,
) -> Result<impl IntoResponse, OmemError> {
    if body.source_space.is_empty() {
        return Err(OmemError::Validation(
            "source_space is required".to_string(),
        ));
    }

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
            "admin access required to manage auto-share rules".to_string(),
        ));
    }

    let rule = AutoShareRule {
        id: Uuid::new_v4().to_string(),
        source_space: body.source_space,
        categories: body.categories.unwrap_or_default(),
        tags: body.tags.unwrap_or_default(),
        min_importance: body.min_importance.unwrap_or(0.0),
        require_approval: body.require_approval.unwrap_or(false),
        created_at: chrono::Utc::now().to_rfc3339(),
    };

    space.auto_share_rules.push(rule.clone());
    space.updated_at = chrono::Utc::now().to_rfc3339();
    state.space_store.update_space(&space).await?;

    Ok((StatusCode::CREATED, Json(rule)))
}

pub async fn list_auto_share_rules(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Path(space_id): Path<String>,
) -> Result<Json<Vec<AutoShareRule>>, OmemError> {
    let space = state
        .space_store
        .get_space(&space_id)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("space {space_id}")))?;

    verify_space_access(&space, &auth.tenant_id)?;

    Ok(Json(space.auto_share_rules))
}

pub async fn delete_auto_share_rule(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Path(path): Path<AutoShareRulePath>,
) -> Result<Json<serde_json::Value>, OmemError> {
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
            "admin access required to manage auto-share rules".to_string(),
        ));
    }

    let before = space.auto_share_rules.len();
    space.auto_share_rules.retain(|r| r.id != path.rule_id);
    if space.auto_share_rules.len() == before {
        return Err(OmemError::NotFound(format!(
            "rule {} in space {}",
            path.rule_id, path.id
        )));
    }

    space.updated_at = chrono::Utc::now().to_rfc3339();
    state.space_store.update_space(&space).await?;

    Ok(Json(serde_json::json!({"status": "deleted"})))
}

pub async fn reshare_memory(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Path(id): Path<String>,
    Json(body): Json<ReshareRequest>,
) -> Result<impl IntoResponse, OmemError> {
    let spaces = state
        .space_store
        .list_spaces_for_user(&auth.tenant_id)
        .await?;

    let mut old_copy: Option<Memory> = None;
    let mut found_store = None;

    if let Some(ref target_space_id) = body.target_space {
        let space = spaces
            .iter()
            .find(|s| s.id == *target_space_id)
            .ok_or_else(|| OmemError::NotFound(format!("space {target_space_id}")))?;
        verify_space_write_access(space, &auth.tenant_id)?;

        let store = state.store_manager.get_store(target_space_id).await?;
        if let Some(mem) = store.get_by_id(&id).await? {
            old_copy = Some(mem);
            found_store = Some(store);
        }
    } else {
        let personal_store = state
            .store_manager
            .get_store(&personal_space_id(&auth.tenant_id))
            .await?;
        if let Some(mem) = personal_store.get_by_id(&id).await? {
            old_copy = Some(mem);
            found_store = Some(personal_store);
        }

        if old_copy.is_none() {
            for space in &spaces {
                let store = state.store_manager.get_store(&space.id).await?;
                if let Some(mem) = store.get_by_id(&id).await? {
                    old_copy = Some(mem);
                    found_store = Some(store);
                    break;
                }
            }
        }
    }

    let old_copy = old_copy.ok_or_else(|| OmemError::NotFound(format!("memory {id}")))?;
    let target_store = found_store.unwrap();

    let provenance = old_copy
        .provenance
        .as_ref()
        .ok_or_else(|| OmemError::Validation("memory is not a shared copy".to_string()))?;

    let source_store = state
        .store_manager
        .get_store(&provenance.shared_from_space)
        .await?;

    let source_memory = source_store
        .get_by_id(&provenance.shared_from_memory)
        .await?
        .ok_or_else(|| {
            OmemError::NotFound(format!(
                "source memory {} no longer exists",
                provenance.shared_from_memory
            ))
        })?;

    let source_vector = source_store
        .get_vector_by_id(&provenance.shared_from_memory)
        .await?;

    let agent_id = auth.agent_id.as_deref().unwrap_or("");
    let new_copy = make_shared_copy(
        &source_memory,
        &old_copy.space_id,
        &auth.tenant_id,
        agent_id,
    );
    target_store
        .create(&new_copy, source_vector.as_deref())
        .await?;

    target_store.soft_delete(&old_copy.id).await?;

    let event = make_sharing_event(
        SharingAction::Reshare,
        &new_copy.id,
        &provenance.shared_from_space,
        &old_copy.space_id,
        &auth.tenant_id,
        agent_id,
        &content_preview(&source_memory.content),
    );
    state.space_store.record_sharing_event(&event).await?;

    Ok((StatusCode::OK, Json(new_copy)))
}

// ── Auto-share check (for ingest pipeline integration) ───────────────

fn matches_share_filters(memory: &Memory, filters: &Option<ShareAllFilters>) -> bool {
    let Some(f) = filters else { return true };
    if let Some(ref cats) = f.categories {
        if !cats.is_empty() && !cats.contains(&memory.category.to_string()) {
            return false;
        }
    }
    if let Some(ref tags) = f.tags {
        if !tags.is_empty() && !tags.iter().any(|t| memory.tags.contains(t)) {
            return false;
        }
    }
    if let Some(min_imp) = f.min_importance {
        if memory.importance < min_imp {
            return false;
        }
    }
    true
}

fn parse_member_role_for_sharing(s: &str) -> Result<MemberRole, OmemError> {
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

pub async fn find_or_create_shared_space(
    user_a: &str,
    user_b: &str,
    space_store: &crate::store::SpaceStore,
) -> Result<(String, bool), OmemError> {
    let spaces = space_store.list_spaces_for_user(user_a).await?;

    for space in &spaces {
        if space.space_type != SpaceType::Team {
            continue;
        }
        let a_is_member =
            space.owner_id == user_a || space.members.iter().any(|m| m.user_id == user_a);
        let b_is_member =
            space.owner_id == user_b || space.members.iter().any(|m| m.user_id == user_b);
        if a_is_member && b_is_member {
            return Ok((space.id.clone(), false));
        }
    }

    let now = chrono::Utc::now().to_rfc3339();
    let a_short = &user_a[..user_a.len().min(8)];
    let b_short = &user_b[..user_b.len().min(8)];
    let space_id = format!("team/{}", Uuid::new_v4());
    let space = Space {
        id: space_id.clone(),
        space_type: SpaceType::Team,
        name: format!("shared-{}-{}", a_short, b_short),
        owner_id: user_a.to_string(),
        members: vec![
            SpaceMember {
                user_id: user_a.to_string(),
                role: MemberRole::Admin,
                joined_at: now.clone(),
            },
            SpaceMember {
                user_id: user_b.to_string(),
                role: MemberRole::Member,
                joined_at: now.clone(),
            },
        ],
        auto_share_rules: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
    };
    space_store.create_space(&space).await?;
    Ok((space_id, true))
}

pub async fn share_all(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Json(body): Json<ShareAllRequest>,
) -> Result<impl IntoResponse, OmemError> {
    let target_space_id = normalize_space_id(&body.target_space);
    if target_space_id.is_empty() {
        return Err(OmemError::Validation(
            "target_space is required".to_string(),
        ));
    }

    let target_space = state
        .space_store
        .get_space(&target_space_id)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("space {}", target_space_id)))?;
    verify_space_write_access(&target_space, &auth.tenant_id)?;

    let source_store = state
        .store_manager
        .get_store(&personal_space_id(&auth.tenant_id))
        .await?;
    let all_memories = source_store.list_all_active().await?;

    let filtered_ids: Vec<String> = all_memories
        .iter()
        .filter(|m| matches_share_filters(m, &body.filters))
        .map(|m| m.id.clone())
        .collect();

    let total = filtered_ids.len();
    if total > 5000 {
        return Err(OmemError::Validation(
            "share-all limited to 5000 memories. Apply stricter filters.".to_string(),
        ));
    }

    let target_store = state.store_manager.get_store(&target_space.id).await?;
    let agent_id = auth.agent_id.as_deref().unwrap_or("").to_string();

    use futures::stream::{self, StreamExt};

    let results: Vec<(bool, bool)> = stream::iter(filtered_ids.into_iter())
        .map(|mem_id| {
            let source_store = source_store.clone();
            let target_store = target_store.clone();
            let space_store = state.space_store.clone();
            let target_space_id = target_space.id.clone();
            let user_id = auth.tenant_id.clone();
            let agent_id = agent_id.clone();
            async move {
                let existing = target_store.find_by_provenance_source(&mem_id).await;
                if let Ok(existing) = existing {
                    if existing.into_iter().next().is_some() {
                        return (false, true);
                    }
                }
                let result = share_single(
                    &source_store,
                    &target_store,
                    &space_store,
                    &mem_id,
                    &target_space_id,
                    &user_id,
                    &agent_id,
                )
                .await;
                match result {
                    Ok(_) => (true, false),
                    Err(_) => (false, false),
                }
            }
        })
        .buffer_unordered(10)
        .collect()
        .await;

    let mut shared = 0;
    let mut skipped_existing = 0;
    let mut failed = 0;
    for (ok, skipped) in results {
        if ok {
            shared += 1;
        } else if skipped {
            skipped_existing += 1;
        } else {
            failed += 1;
        }
    }

    Ok(Json(ShareAllResponse {
        total,
        shared,
        skipped_existing,
        failed,
    }))
}

pub async fn share_to_user(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Path(id): Path<String>,
    Json(body): Json<ShareToUserRequest>,
) -> Result<impl IntoResponse, OmemError> {
    if body.target_user.is_empty() {
        return Err(OmemError::Validation("target_user is required".to_string()));
    }
    if body.target_user == auth.tenant_id {
        return Err(OmemError::Validation(
            "cannot share to yourself".to_string(),
        ));
    }

    let source_store = state
        .store_manager
        .get_store(&personal_space_id(&auth.tenant_id))
        .await?;
    let source_memory = source_store
        .get_by_id(&id)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("memory {id}")))?;

    let (space_id, space_created) =
        find_or_create_shared_space(&auth.tenant_id, &body.target_user, &state.space_store).await?;

    let target_store = state.store_manager.get_store(&space_id).await?;

    let existing = target_store
        .find_by_provenance_source(&source_memory.id)
        .await?;
    if let Some(copy) = existing.into_iter().next() {
        return Ok((
            StatusCode::OK,
            Json(ShareToUserResponse {
                space_id,
                shared_copy_id: copy.id,
                space_created,
            }),
        ));
    }

    let agent_id = auth.agent_id.as_deref().unwrap_or("");
    let source_vector = source_store.get_vector_by_id(&source_memory.id).await?;
    let copy = make_shared_copy(&source_memory, &space_id, &auth.tenant_id, agent_id);
    let copy_id = copy.id.clone();
    target_store.create(&copy, source_vector.as_deref()).await?;

    let event = make_sharing_event(
        SharingAction::Share,
        &copy_id,
        &source_memory.space_id,
        &space_id,
        &auth.tenant_id,
        agent_id,
        &content_preview(&source_memory.content),
    );
    state.space_store.record_sharing_event(&event).await?;

    Ok((
        StatusCode::CREATED,
        Json(ShareToUserResponse {
            space_id,
            shared_copy_id: copy_id,
            space_created,
        }),
    ))
}

pub async fn share_all_to_user(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Json(body): Json<ShareAllToUserRequest>,
) -> Result<Json<ShareAllToUserResponse>, OmemError> {
    if body.target_user.is_empty() {
        return Err(OmemError::Validation("target_user is required".to_string()));
    }
    if body.target_user == auth.tenant_id {
        return Err(OmemError::Validation(
            "cannot share to yourself".to_string(),
        ));
    }

    let (space_id, space_created) =
        find_or_create_shared_space(&auth.tenant_id, &body.target_user, &state.space_store).await?;

    let source_store = state
        .store_manager
        .get_store(&personal_space_id(&auth.tenant_id))
        .await?;
    let all_memories = source_store.list_all_active().await?;
    let filtered_ids: Vec<String> = all_memories
        .iter()
        .filter(|m| matches_share_filters(m, &body.filters))
        .map(|m| m.id.clone())
        .collect();

    let total = filtered_ids.len();
    if total > 5000 {
        return Err(OmemError::Validation(
            "share-all-to-user limited to 5000 memories. Apply stricter filters.".to_string(),
        ));
    }

    let target_store = state.store_manager.get_store(&space_id).await?;
    let agent_id = auth.agent_id.as_deref().unwrap_or("").to_string();

    use futures::stream::{self, StreamExt};

    let results: Vec<(bool, bool)> = stream::iter(filtered_ids.into_iter())
        .map(|mem_id| {
            let source_store = source_store.clone();
            let target_store = target_store.clone();
            let space_store = state.space_store.clone();
            let space_id = space_id.clone();
            let user_id = auth.tenant_id.clone();
            let agent_id = agent_id.clone();
            async move {
                let existing = target_store.find_by_provenance_source(&mem_id).await;
                if let Ok(existing) = existing {
                    if existing.into_iter().next().is_some() {
                        return (false, true);
                    }
                }
                let result = share_single(
                    &source_store,
                    &target_store,
                    &space_store,
                    &mem_id,
                    &space_id,
                    &user_id,
                    &agent_id,
                )
                .await;
                match result {
                    Ok(_) => (true, false),
                    Err(_) => (false, false),
                }
            }
        })
        .buffer_unordered(10)
        .collect()
        .await;

    let mut shared = 0;
    let mut skipped_existing = 0;
    let mut failed = 0;
    for (ok, skipped) in results {
        if ok {
            shared += 1;
        } else if skipped {
            skipped_existing += 1;
        } else {
            failed += 1;
        }
    }

    Ok(Json(ShareAllToUserResponse {
        space_id,
        space_created,
        total,
        shared,
        skipped_existing,
        failed,
    }))
}

pub async fn org_setup(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Json(body): Json<OrgSetupRequest>,
) -> Result<impl IntoResponse, OmemError> {
    if body.name.is_empty() {
        return Err(OmemError::Validation("name is required".to_string()));
    }

    let now = chrono::Utc::now().to_rfc3339();
    let space_id = format!("org/{}", Uuid::new_v4());

    let mut members = vec![SpaceMember {
        user_id: auth.tenant_id.clone(),
        role: MemberRole::Admin,
        joined_at: now.clone(),
    }];

    let mut members_added = 0;
    let mut failed_members = Vec::new();

    for m in &body.members {
        if m.user_id == auth.tenant_id {
            continue;
        }
        match parse_member_role_for_sharing(&m.role) {
            Ok(role) => {
                members.push(SpaceMember {
                    user_id: m.user_id.clone(),
                    role,
                    joined_at: now.clone(),
                });
                members_added += 1;
            }
            Err(_) => {
                failed_members.push(m.user_id.clone());
            }
        }
    }

    let space = Space {
        id: space_id.clone(),
        space_type: SpaceType::Organization,
        name: body.name.clone(),
        owner_id: auth.tenant_id.clone(),
        members,
        auto_share_rules: Vec::new(),
        created_at: now.clone(),
        updated_at: now,
    };

    state.space_store.create_space(&space).await?;

    Ok((
        StatusCode::CREATED,
        Json(OrgSetupResponse {
            space_id,
            name: body.name,
            members_added,
            failed_members,
        }),
    ))
}

pub async fn org_publish(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Path(org_id): Path<String>,
    Json(body): Json<OrgPublishRequest>,
) -> Result<Json<OrgPublishResponse>, OmemError> {
    let org_id = normalize_space_id(&org_id);
    let mut space = state
        .space_store
        .get_space(&org_id)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("org space {org_id}")))?;

    if space.space_type != SpaceType::Organization {
        return Err(OmemError::Validation(format!(
            "space {} is not an organization",
            org_id
        )));
    }

    let is_admin = space.owner_id == auth.tenant_id
        || space
            .members
            .iter()
            .any(|m| m.user_id == auth.tenant_id && m.role == MemberRole::Admin);
    if !is_admin {
        return Err(OmemError::Unauthorized(
            "org admin access required to publish".to_string(),
        ));
    }

    let mut published = 0;
    let mut skipped_existing = 0;
    let mut failed = 0;

    if let Some(memory_ids) = &body.memory_ids {
        if memory_ids.len() > 500 {
            return Err(OmemError::Validation(
                "org/publish limited to 500 memories per call".to_string(),
            ));
        }

        let source_store = state
            .store_manager
            .get_store(&personal_space_id(&auth.tenant_id))
            .await?;
        let target_store = state.store_manager.get_store(&org_id).await?;
        let agent_id = auth.agent_id.as_deref().unwrap_or("").to_string();

        use futures::stream::{self, StreamExt};

        let results: Vec<(bool, bool)> = stream::iter(memory_ids.clone().into_iter())
            .map(|mem_id| {
                let source_store = source_store.clone();
                let target_store = target_store.clone();
                let space_store = state.space_store.clone();
                let org_id = org_id.clone();
                let user_id = auth.tenant_id.clone();
                let agent_id = agent_id.clone();
                async move {
                    let existing = target_store.find_by_provenance_source(&mem_id).await;
                    if let Ok(existing) = existing {
                        if existing.into_iter().next().is_some() {
                            return (false, true);
                        }
                    }
                    let result = share_single(
                        &source_store,
                        &target_store,
                        &space_store,
                        &mem_id,
                        &org_id,
                        &user_id,
                        &agent_id,
                    )
                    .await;
                    match result {
                        Ok(_) => (true, false),
                        Err(_) => (false, false),
                    }
                }
            })
            .buffer_unordered(10)
            .collect()
            .await;

        for (ok, skipped) in results {
            if ok {
                published += 1;
            } else if skipped {
                skipped_existing += 1;
            } else {
                failed += 1;
            }
        }
    }

    let mut auto_share_rule_id = None;
    if let Some(rule_req) = &body.auto_share_rule {
        let rule = AutoShareRule {
            id: Uuid::new_v4().to_string(),
            source_space: personal_space_id(&auth.tenant_id),
            categories: rule_req.categories.clone().unwrap_or_default(),
            tags: Vec::new(),
            min_importance: rule_req.min_importance.unwrap_or(0.0),
            require_approval: false,
            created_at: chrono::Utc::now().to_rfc3339(),
        };
        auto_share_rule_id = Some(rule.id.clone());
        space.auto_share_rules.push(rule);
        space.updated_at = chrono::Utc::now().to_rfc3339();
        state.space_store.update_space(&space).await?;
    }

    Ok(Json(OrgPublishResponse {
        published,
        skipped_existing,
        failed,
        auto_share_rule_id,
    }))
}

pub async fn check_auto_share(
    memory: &Memory,
    space_store: &crate::store::SpaceStore,
    store_manager: &StoreManager,
    user_id: &str,
    agent_id: &str,
) -> Result<Vec<String>, OmemError> {
    let spaces = space_store.list_spaces_for_user(user_id).await?;
    let mut shared_to = Vec::new();

    for space in &spaces {
        for rule in &space.auto_share_rules {
            if rule.source_space != memory.space_id {
                continue;
            }
            if !rule.categories.is_empty()
                && !rule.categories.contains(&memory.category.to_string())
            {
                continue;
            }
            if !rule.tags.is_empty() && !rule.tags.iter().any(|t| memory.tags.contains(t)) {
                continue;
            }
            if memory.importance < rule.min_importance {
                continue;
            }
            if rule.require_approval {
                continue;
            }

            let target_store = store_manager.get_store(&space.id).await?;
            let source_store = store_manager.get_store(&memory.space_id).await?;
            let source_vector = source_store.get_vector_by_id(&memory.id).await?;
            let copy = make_shared_copy(memory, &space.id, user_id, agent_id);
            target_store.create(&copy, source_vector.as_deref()).await?;

            let event = make_sharing_event(
                SharingAction::Share,
                &copy.id,
                &memory.space_id,
                &space.id,
                user_id,
                agent_id,
                &content_preview(&memory.content),
            );
            space_store.record_sharing_event(&event).await?;

            shared_to.push(space.id.clone());
            break;
        }
    }

    Ok(shared_to)
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::category::Category;
    use crate::domain::space::{MemberRole, Space, SpaceMember, SpaceType};
    use crate::domain::types::MemoryType;
    use crate::store::{SpaceStore, StoreManager};
    use tempfile::TempDir;

    struct TestEnv {
        store_manager: StoreManager,
        space_store: SpaceStore,
        _dir: TempDir,
        _space_dir: TempDir,
    }

    async fn setup() -> TestEnv {
        let dir = TempDir::new().expect("temp dir");
        let space_dir = TempDir::new().expect("space temp dir");

        let store_manager = StoreManager::new(dir.path().to_str().expect("path"));
        let space_store = SpaceStore::new(space_dir.path().to_str().expect("path"))
            .await
            .expect("space store");
        space_store.init_tables().await.expect("init tables");

        TestEnv {
            store_manager,
            space_store,
            _dir: dir,
            _space_dir: space_dir,
        }
    }

    fn make_space(id: &str, owner: &str) -> Space {
        Space {
            id: id.to_string(),
            space_type: SpaceType::Team,
            name: id.to_string(),
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

    fn make_memory(content: &str, tenant: &str, space_id: &str) -> Memory {
        let mut mem = Memory::new(content, Category::Preferences, MemoryType::Insight, tenant);
        mem.space_id = space_id.to_string();
        mem.owner_agent_id = "agent-1".to_string();
        mem
    }

    #[tokio::test]
    async fn test_share_memory() {
        let env = setup().await;
        let personal_store = env
            .store_manager
            .get_store("user-001")
            .await
            .expect("personal store");
        let team_space = make_space("team:backend", "user-001");
        env.space_store
            .create_space(&team_space)
            .await
            .expect("create space");

        let mem = make_memory("user prefers dark mode", "user-001", "user-001");
        personal_store.create(&mem, None).await.expect("create mem");

        let target_store = env
            .store_manager
            .get_store("team:backend")
            .await
            .expect("target store");

        let copy = make_shared_copy(&mem, "team:backend", "user-001", "agent-1");
        target_store.create(&copy, None).await.expect("create copy");

        let fetched = target_store
            .get_by_id(&copy.id)
            .await
            .expect("get")
            .expect("exists");
        assert_eq!(fetched.content, "user prefers dark mode");
        assert_eq!(fetched.space_id, "team:backend");
        assert_eq!(fetched.visibility, "global");
        assert!(fetched.provenance.is_some());
        let prov = fetched.provenance.expect("provenance");
        assert_eq!(prov.shared_from_memory, mem.id);
        assert_eq!(prov.shared_from_space, "user-001");
        assert_eq!(prov.shared_by_user, "user-001");
    }

    #[tokio::test]
    async fn test_pull_memory() {
        let env = setup().await;
        let team_space = make_space("team:backend", "user-001");
        env.space_store
            .create_space(&team_space)
            .await
            .expect("create space");

        let team_store = env
            .store_manager
            .get_store("team:backend")
            .await
            .expect("team store");
        let mem = make_memory("architecture: use hexagonal", "user-001", "team:backend");
        team_store.create(&mem, None).await.expect("create");

        let personal_store = env
            .store_manager
            .get_store("user-001")
            .await
            .expect("personal store");
        let copy = make_shared_copy(&mem, "user-001", "user-001", "agent-1");
        personal_store.create(&copy, None).await.expect("pull");

        let fetched = personal_store
            .get_by_id(&copy.id)
            .await
            .expect("get")
            .expect("exists");
        assert_eq!(fetched.content, "architecture: use hexagonal");
        assert!(fetched.provenance.is_some());
        let prov = fetched.provenance.expect("provenance");
        assert_eq!(prov.shared_from_space, "team:backend");
        assert_eq!(prov.shared_from_memory, mem.id);
    }

    #[tokio::test]
    async fn test_unshare_memory() {
        let env = setup().await;
        let team_space = make_space("team:backend", "user-001");
        env.space_store
            .create_space(&team_space)
            .await
            .expect("create space");

        let personal_store = env
            .store_manager
            .get_store("user-001")
            .await
            .expect("personal store");
        let mem = make_memory("secret data", "user-001", "user-001");
        personal_store.create(&mem, None).await.expect("create");

        let target_store = env
            .store_manager
            .get_store("team:backend")
            .await
            .expect("target store");
        let copy = make_shared_copy(&mem, "team:backend", "user-001", "agent-1");
        target_store.create(&copy, None).await.expect("share");

        target_store.soft_delete(&copy.id).await.expect("unshare");

        let deleted = target_store
            .get_by_id(&copy.id)
            .await
            .expect("get")
            .expect("exists");
        assert_eq!(deleted.state, crate::domain::types::MemoryState::Deleted);
    }

    #[tokio::test]
    async fn test_batch_share() {
        let env = setup().await;
        let team_space = make_space("team:backend", "user-001");
        env.space_store
            .create_space(&team_space)
            .await
            .expect("create space");

        let personal_store = env
            .store_manager
            .get_store("user-001")
            .await
            .expect("personal store");
        let target_store = env
            .store_manager
            .get_store("team:backend")
            .await
            .expect("target store");

        let mut mems = Vec::new();
        for i in 0..3 {
            let mem = make_memory(&format!("batch memory {i}"), "user-001", "user-001");
            personal_store.create(&mem, None).await.expect("create");
            mems.push(mem);
        }

        for mem in &mems {
            let copy = make_shared_copy(mem, "team:backend", "user-001", "agent-1");
            target_store.create(&copy, None).await.expect("batch share");
        }

        let team_list = target_store.list(100, 0).await.expect("list");
        assert_eq!(team_list.len(), 3);
    }

    #[tokio::test]
    async fn test_auto_share_rule() {
        let env = setup().await;
        let mut team_space = make_space("team:backend", "user-001");
        let rule = AutoShareRule {
            id: "rule-1".to_string(),
            source_space: "user-001".to_string(),
            categories: vec!["preferences".to_string()],
            tags: Vec::new(),
            min_importance: 0.3,
            require_approval: false,
            created_at: "2025-01-01T00:00:00Z".to_string(),
        };
        team_space.auto_share_rules.push(rule);
        env.space_store
            .create_space(&team_space)
            .await
            .expect("create");

        let personal_store = env
            .store_manager
            .get_store("user-001")
            .await
            .expect("personal store");
        let mem = make_memory("prefers vim keybindings", "user-001", "user-001");
        personal_store.create(&mem, None).await.expect("create");

        let shared_to = check_auto_share(
            &mem,
            &env.space_store,
            &env.store_manager,
            "user-001",
            "agent-1",
        )
        .await
        .expect("auto share");
        assert_eq!(shared_to, vec!["team:backend"]);

        let team_store = env
            .store_manager
            .get_store("team:backend")
            .await
            .expect("team store");
        let team_list = team_store.list(100, 0).await.expect("list");
        assert_eq!(team_list.len(), 1);
        assert_eq!(team_list[0].content, "prefers vim keybindings");
    }

    #[tokio::test]
    async fn test_sharing_events_recorded() {
        let env = setup().await;
        let team_space = make_space("team:backend", "user-001");
        env.space_store
            .create_space(&team_space)
            .await
            .expect("create space");

        let event = make_sharing_event(
            SharingAction::Share,
            "mem-001",
            "user-001",
            "team:backend",
            "user-001",
            "agent-1",
            "user prefers dark mode",
        );
        env.space_store
            .record_sharing_event(&event)
            .await
            .expect("record event");

        let events = env
            .space_store
            .list_sharing_events("team:backend", 100)
            .await
            .expect("list events");
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].action, SharingAction::Share);
        assert_eq!(events[0].from_space, "user-001");
        assert_eq!(events[0].to_space, "team:backend");
    }

    #[tokio::test]
    async fn test_version_auto_increment() {
        let env = setup().await;
        let store = env
            .store_manager
            .get_store("user-001")
            .await
            .expect("store");

        let mem = make_memory("original content", "user-001", "user-001");
        store.create(&mem, None).await.expect("create");

        let created = store
            .get_by_id(&mem.id)
            .await
            .expect("get")
            .expect("exists");
        assert_eq!(created.version, Some(1));

        let mut updated = created;
        updated.content = "updated content".to_string();
        store.update(&updated, None).await.expect("update");

        let fetched = store
            .get_by_id(&mem.id)
            .await
            .expect("get")
            .expect("exists");
        assert_eq!(fetched.version, Some(2));
        assert_eq!(fetched.content, "updated content");

        let mut updated2 = fetched;
        updated2.content = "second update".to_string();
        store.update(&updated2, None).await.expect("update2");

        let fetched2 = store
            .get_by_id(&mem.id)
            .await
            .expect("get")
            .expect("exists");
        assert_eq!(fetched2.version, Some(3));
    }

    #[tokio::test]
    async fn test_stale_detection() {
        use crate::api::handlers::memory::check_stale_for_memory;

        let env = setup().await;
        let source_store = env
            .store_manager
            .get_store("user-001")
            .await
            .expect("source store");

        let mem = make_memory("dark mode preference", "user-001", "user-001");
        source_store.create(&mem, None).await.expect("create");

        let team_space = make_space("team:backend", "user-001");
        env.space_store
            .create_space(&team_space)
            .await
            .expect("create space");

        let target_store = env
            .store_manager
            .get_store("team:backend")
            .await
            .expect("target store");

        let copy = make_shared_copy(&mem, "team:backend", "user-001", "agent-1");
        target_store.create(&copy, None).await.expect("create copy");

        let stale = check_stale_for_memory(&copy, &env.store_manager).await;
        assert!(stale.is_some());
        let info = stale.unwrap();
        assert!(!info.is_stale);
        assert_eq!(info.source_version, Some(1));
        assert_eq!(info.current_source_version, Some(1));
        assert!(!info.source_deleted);

        let mut updated_source = mem.clone();
        updated_source.content = "light mode preference".to_string();
        source_store
            .update(&updated_source, None)
            .await
            .expect("update source");

        let stale2 = check_stale_for_memory(&copy, &env.store_manager).await;
        assert!(stale2.is_some());
        let info2 = stale2.unwrap();
        assert!(info2.is_stale);
        assert_eq!(info2.source_version, Some(1));
        assert_eq!(info2.current_source_version, Some(2));
    }

    #[tokio::test]
    async fn test_reshare_updates_copy() {
        let env = setup().await;
        let source_store = env
            .store_manager
            .get_store("user-001")
            .await
            .expect("source store");

        let mem = make_memory("original fact", "user-001", "user-001");
        source_store.create(&mem, None).await.expect("create");

        let team_space = make_space("team:backend", "user-001");
        env.space_store
            .create_space(&team_space)
            .await
            .expect("create space");

        let target_store = env
            .store_manager
            .get_store("team:backend")
            .await
            .expect("target store");

        let copy = make_shared_copy(&mem, "team:backend", "user-001", "agent-1");
        let copy_id = copy.id.clone();
        target_store.create(&copy, None).await.expect("create copy");

        let mut updated_source = mem.clone();
        updated_source.content = "updated fact".to_string();
        source_store
            .update(&updated_source, None)
            .await
            .expect("update");

        let source_after = source_store
            .get_by_id(&mem.id)
            .await
            .expect("get")
            .expect("exists");
        let new_copy = make_shared_copy(&source_after, "team:backend", "user-001", "agent-1");
        target_store
            .create(&new_copy, None)
            .await
            .expect("create new copy");
        target_store
            .soft_delete(&copy_id)
            .await
            .expect("delete old");

        let new_fetched = target_store
            .get_by_id(&new_copy.id)
            .await
            .expect("get")
            .expect("exists");
        assert_eq!(new_fetched.content, "updated fact");
        assert_eq!(new_fetched.version, Some(1));
        let prov = new_fetched.provenance.expect("provenance");
        assert_eq!(prov.source_version, Some(2));
        assert_eq!(prov.shared_from_memory, mem.id);

        let old = target_store.get_by_id(&copy_id).await.expect("get");
        assert!(old.is_some());
        assert_eq!(
            old.unwrap().state,
            crate::domain::types::MemoryState::Deleted
        );
    }

    #[tokio::test]
    async fn test_share_all_with_filters() {
        let env = setup().await;
        let team_space = make_space("team/backend", "user-001");
        env.space_store
            .create_space(&team_space)
            .await
            .expect("create space");

        let personal_store = env
            .store_manager
            .get_store("user-001")
            .await
            .expect("personal store");

        let mem1 = make_memory("preference: dark mode", "user-001", "user-001");
        personal_store
            .create(&mem1, None)
            .await
            .expect("create mem1");

        let mut mem2 = make_memory("some random fact", "user-001", "user-001");
        mem2.category = crate::domain::category::Category::Events;
        personal_store
            .create(&mem2, None)
            .await
            .expect("create mem2");

        let target_store = env
            .store_manager
            .get_store("team/backend")
            .await
            .expect("target store");

        let agent_id = "agent-1";
        let user_id = "user-001";

        let all = personal_store.list_all_active().await.expect("list");
        assert_eq!(all.len(), 2);

        let filters = Some(ShareAllFilters {
            categories: Some(vec!["preferences".to_string()]),
            tags: None,
            min_importance: None,
        });
        let filtered_ids: Vec<String> = all
            .iter()
            .filter(|m| matches_share_filters(m, &filters))
            .map(|m| m.id.clone())
            .collect();
        assert_eq!(filtered_ids.len(), 1);

        for mid in &filtered_ids {
            let _ = share_single(
                &personal_store,
                &target_store,
                &env.space_store,
                mid,
                "team/backend",
                user_id,
                agent_id,
            )
            .await
            .expect("share single");
        }

        let team_list = target_store.list(100, 0).await.expect("list");
        assert_eq!(team_list.len(), 1);
        assert!(team_list[0].content.contains("dark mode"));
    }

    #[tokio::test]
    async fn test_find_or_create_shared_space_creates_new() {
        let env = setup().await;
        let (space_id, created) = find_or_create_shared_space("alice", "bob", &env.space_store)
            .await
            .expect("create shared space");
        assert!(created);
        assert!(space_id.starts_with("team/"));

        let space = env
            .space_store
            .get_space(&space_id)
            .await
            .expect("get")
            .expect("exists");
        assert_eq!(space.space_type, SpaceType::Team);
        assert_eq!(space.owner_id, "alice");
        assert!(space.members.iter().any(|m| m.user_id == "bob"));
    }

    #[tokio::test]
    async fn test_find_or_create_shared_space_reuses_existing() {
        let env = setup().await;
        let (space_id1, created1) = find_or_create_shared_space("alice", "bob", &env.space_store)
            .await
            .expect("first call");
        assert!(created1);

        let (space_id2, created2) = find_or_create_shared_space("alice", "bob", &env.space_store)
            .await
            .expect("second call");
        assert!(!created2);
        assert_eq!(space_id1, space_id2);
    }

    #[tokio::test]
    async fn test_org_setup_creates_space_with_members() {
        let env = setup().await;

        let now = chrono::Utc::now().to_rfc3339();
        let space_id = format!("org/{}", Uuid::new_v4());
        let space = Space {
            id: space_id.clone(),
            space_type: SpaceType::Organization,
            name: "Acme Corp".to_string(),
            owner_id: "admin-user".to_string(),
            members: vec![
                SpaceMember {
                    user_id: "admin-user".to_string(),
                    role: MemberRole::Admin,
                    joined_at: now.clone(),
                },
                SpaceMember {
                    user_id: "reader-user".to_string(),
                    role: MemberRole::Reader,
                    joined_at: now.clone(),
                },
            ],
            auto_share_rules: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
        };
        env.space_store
            .create_space(&space)
            .await
            .expect("create org space");

        let fetched = env
            .space_store
            .get_space(&space_id)
            .await
            .expect("get")
            .expect("exists");
        assert_eq!(fetched.space_type, SpaceType::Organization);
        assert_eq!(fetched.name, "Acme Corp");
        assert_eq!(fetched.members.len(), 2);
        assert!(space_id.starts_with("org/"));
    }

    #[tokio::test]
    async fn test_org_publish_permission_check() {
        let env = setup().await;
        let now = chrono::Utc::now().to_rfc3339();
        let space_id = format!("org/{}", Uuid::new_v4());
        let space = Space {
            id: space_id.clone(),
            space_type: SpaceType::Organization,
            name: "Acme Corp".to_string(),
            owner_id: "admin-user".to_string(),
            members: vec![
                SpaceMember {
                    user_id: "admin-user".to_string(),
                    role: MemberRole::Admin,
                    joined_at: now.clone(),
                },
                SpaceMember {
                    user_id: "reader-user".to_string(),
                    role: MemberRole::Reader,
                    joined_at: now.clone(),
                },
            ],
            auto_share_rules: Vec::new(),
            created_at: now.clone(),
            updated_at: now,
        };
        env.space_store
            .create_space(&space)
            .await
            .expect("create org space");

        let fetched = env
            .space_store
            .get_space(&space_id)
            .await
            .expect("get")
            .expect("exists");

        let is_admin_for_admin = fetched.owner_id == "admin-user"
            || fetched
                .members
                .iter()
                .any(|m| m.user_id == "admin-user" && m.role == MemberRole::Admin);
        assert!(is_admin_for_admin);

        let is_admin_for_reader = fetched.owner_id == "reader-user"
            || fetched
                .members
                .iter()
                .any(|m| m.user_id == "reader-user" && m.role == MemberRole::Admin);
        assert!(!is_admin_for_reader);
    }

    #[tokio::test]
    async fn test_space_id_uses_slash_format() {
        let now = chrono::Utc::now().to_rfc3339();
        let team_id = format!("team/{}", Uuid::new_v4());
        assert!(team_id.starts_with("team/"));
        assert!(!team_id.contains(':'));

        let org_id = format!("org/{}", Uuid::new_v4());
        assert!(org_id.starts_with("org/"));
        assert!(!org_id.contains(':'));

        let personal_id = crate::api::server::personal_space_id("alice");
        assert_eq!(personal_id, "personal/alice");
        assert!(!personal_id.contains(':'));

        let normalized = crate::api::server::normalize_space_id("team:old-uuid");
        assert_eq!(normalized, "team/old-uuid");

        let already_good = crate::api::server::normalize_space_id("team/new-uuid");
        assert_eq!(already_good, "team/new-uuid");

        let unknown = crate::api::server::normalize_space_id("custom:something");
        assert_eq!(unknown, "custom:something");

        let _ = now;
    }
}
