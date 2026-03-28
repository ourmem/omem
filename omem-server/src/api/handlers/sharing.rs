use std::sync::Arc;

use axum::extract::{Extension, Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::api::server::{AppState, personal_space_id};
use crate::domain::error::OmemError;
use crate::domain::memory::Memory;
use crate::domain::space::{AutoShareRule, MemberRole, Provenance, SharingAction, SharingEvent, Space};
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
pub struct AutoShareRulePath {
    pub id: String,
    pub rule_id: String,
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

fn make_shared_copy(
    source: &Memory,
    target_space: &str,
    user_id: &str,
    agent_id: &str,
) -> Memory {
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
        }),
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

    let source_store = state.store_manager.get_store(&personal_space_id(&auth.tenant_id)).await?;
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

    let agent_id = auth.agent_id.as_deref().unwrap_or("");
    let copy = make_shared_copy(&source_memory, &target_space.id, &auth.tenant_id, agent_id);
    target_store.create(&copy, None).await?;

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
    let source_memory = source_store
        .get_by_id(&id)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("memory {id} in space {}", body.source_space)))?;

    let personal_store = state.store_manager.get_store(&personal_space_id(&auth.tenant_id)).await?;
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
    });

    personal_store.create(&copy, None).await?;

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

    let source_store = state.store_manager.get_store(&personal_space_id(&auth.tenant_id)).await?;
    let target_store = state.store_manager.get_store(&target_space.id).await?;
    let agent_id = auth.agent_id.as_deref().unwrap_or("");

    let mut succeeded = Vec::new();
    let mut failed = Vec::new();

    for mem_id in &body.memory_ids {
        match share_single(
            &source_store,
            &target_store,
            &state.space_store,
            mem_id,
            &target_space.id,
            &auth.tenant_id,
            agent_id,
        )
        .await
        {
            Ok(copy) => succeeded.push(copy),
            Err(e) => failed.push(BatchShareError {
                memory_id: mem_id.clone(),
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

    let copy = make_shared_copy(&source, target_space_id, user_id, agent_id);
    target_store.create(&copy, None).await?;

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

// ── Auto-share check (for ingest pipeline integration) ───────────────

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
            let copy = make_shared_copy(memory, &space.id, user_id, agent_id);
            target_store.create(&copy, None).await?;

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
    use crate::store::{LanceStore, SpaceStore, StoreManager};
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
            let mem = make_memory(
                &format!("batch memory {i}"),
                "user-001",
                "user-001",
            );
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
}
