use std::sync::Arc;

use axum::extract::{Extension, Path, Query, State};
use axum::Json;
use axum_extra::extract::Multipart;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::api::server::{personal_space_id, AppState};
use crate::domain::error::OmemError;
use crate::domain::relation::{MemoryRelation, RelationType};
use crate::domain::tenant::AuthInfo;
use crate::ingest::intelligence::IntelligenceTask;
use crate::ingest::session::{SessionMessage, SessionStore};
use crate::store::spaces::ImportTaskRecord;

#[derive(Deserialize)]
pub struct ListImportsQuery {
    #[serde(default = "default_import_limit")]
    pub limit: usize,
}

fn default_import_limit() -> usize {
    50
}

pub async fn create_import(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    mut multipart: Multipart,
) -> Result<Json<ImportTaskRecord>, OmemError> {
    let mut file_data: Option<Vec<u8>> = None;
    let mut filename = String::new();
    let mut file_type = String::from("memory");
    let mut agent_id: Option<String> = None;
    let mut session_id: Option<String> = None;
    let mut space_id: Option<String> = None;
    let mut post_process = true;
    let mut force = false;
    let mut strategy = String::from("auto");

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| OmemError::Validation(format!("multipart error: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        match name.as_str() {
            "file" => {
                filename = field.file_name().unwrap_or("unknown").to_string();
                file_data = Some(
                    field
                        .bytes()
                        .await
                        .map_err(|e| OmemError::Validation(format!("read file: {e}")))?
                        .to_vec(),
                );
            }
            "file_type" => {
                file_type = field
                    .text()
                    .await
                    .map_err(|e| OmemError::Validation(format!("{e}")))?;
            }
            "agent_id" => {
                agent_id = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| OmemError::Validation(format!("{e}")))?,
                );
            }
            "session_id" => {
                session_id = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| OmemError::Validation(format!("{e}")))?,
                );
            }
            "space_id" => {
                space_id = Some(
                    field
                        .text()
                        .await
                        .map_err(|e| OmemError::Validation(format!("{e}")))?,
                );
            }
            "post_process" => {
                let val = field
                    .text()
                    .await
                    .map_err(|e| OmemError::Validation(format!("{e}")))?;
                post_process = val != "false" && val != "0";
            }
            "force" => {
                let val = field
                    .text()
                    .await
                    .map_err(|e| OmemError::Validation(format!("{e}")))?;
                force = val == "true" || val == "1";
            }
            "strategy" => {
                strategy = field
                    .text()
                    .await
                    .map_err(|e| OmemError::Validation(format!("{e}")))?;
            }
            _ => {}
        }
    }

    let data = file_data.ok_or_else(|| OmemError::Validation("no 'file' field".to_string()))?;
    let content =
        String::from_utf8(data).map_err(|_| OmemError::Validation("not valid UTF-8".to_string()))?;

    let valid_types = ["memory", "session", "markdown", "jsonl"];
    if !valid_types.contains(&file_type.as_str()) {
        return Err(OmemError::Validation(format!(
            "unsupported file_type: {file_type}. Use: memory, session, markdown, jsonl"
        )));
    }

    let valid_strategies = ["auto", "atomic", "section", "document"];
    if !valid_strategies.contains(&strategy.as_str()) {
        return Err(OmemError::Validation(format!(
            "unsupported strategy: {strategy}. Use: auto, atomic, section, document"
        )));
    }

    let target_space = space_id.unwrap_or_else(|| personal_space_id(&auth.tenant_id));
    let store = state.store_manager.get_store(&target_space).await?;
    let task_id = Uuid::new_v4().to_string();
    let now = chrono::Utc::now().to_rfc3339();

    let content_hash = sha256_hex(&content);

    let session_uri = format!("{}/{}", state.config.store_uri(), target_space);
    let session_store = Arc::new(
        SessionStore::new(&session_uri)
            .await
            .map_err(|e| OmemError::Storage(format!("session store: {e}")))?,
    );
    session_store.init_table().await?;

    if !force && session_store.exists_by_hash(&content_hash).await? {
        return Err(OmemError::Validation(
            "file already imported (duplicate content)".to_string(),
        ));
    }

    let import_session_id = format!("import-{}", task_id);
    let session_msg = SessionMessage {
        id: Uuid::new_v4().to_string(),
        session_id: import_session_id,
        agent_id: agent_id.clone().unwrap_or_default(),
        role: "import".to_string(),
        content,
        content_hash,
        tags: vec![
            format!("file_type:{}", file_type),
            format!("filename:{}", filename),
        ],
        created_at: now.clone(),
    };

    let _sid = session_id;

    session_store.bulk_create(&[session_msg]).await?;

    let task = ImportTaskRecord {
        id: task_id.clone(),
        status: if post_process {
            "processing".to_string()
        } else {
            "completed".to_string()
        },
        file_type: file_type.clone(),
        filename: filename.clone(),
        agent_id: agent_id.clone(),
        space_id: target_space.clone(),
        post_process,
        strategy: strategy.clone(),
        storage_total: 1,
        storage_stored: 1,
        storage_skipped: 0,
        extraction_status: if post_process {
            "pending".to_string()
        } else {
            "skipped".to_string()
        },
        extraction_chunks: 0,
        extraction_facts: 0,
        extraction_progress: 0,
        reconcile_status: if post_process {
            "pending".to_string()
        } else {
            "skipped".to_string()
        },
        reconcile_relations: 0,
        reconcile_merged: 0,
        reconcile_progress: 0,
        errors: Vec::new(),
        created_at: now,
        completed_at: if post_process {
            None
        } else {
            Some(chrono::Utc::now().to_rfc3339())
        },
    };

    state.space_store.create_import_task(&task).await?;

    if post_process {
        let bg_store = store;
        let bg_session_store = session_store;
        let bg_embed = state.embed.clone();
        let bg_llm = state.llm.clone();
        let bg_space_store = state.space_store.clone();
        let bg_task_id = task_id;
        let bg_tenant_id = auth.tenant_id.clone();
        let bg_strategy = strategy;
        let sem = state.import_semaphore.clone();
        let reconcile_sem = state.reconcile_semaphore.clone();

        tokio::spawn(async move {
            let _permit = sem.acquire_owned().await.expect("semaphore");
            let intelligence = IntelligenceTask::new(
                bg_store,
                bg_session_store,
                bg_embed,
                bg_llm,
                bg_space_store,
                reconcile_sem,
                bg_task_id.clone(),
                bg_tenant_id,
                bg_strategy,
            );
            intelligence.run().await;
        });
    }

    Ok(Json(task))
}

pub async fn list_imports(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Query(params): Query<ListImportsQuery>,
) -> Result<Json<serde_json::Value>, OmemError> {
    let space_id = personal_space_id(&auth.tenant_id);
    let tasks = state
        .space_store
        .list_import_tasks(&space_id, params.limit)
        .await
        .unwrap_or_default();

    Ok(Json(serde_json::json!({
        "imports": tasks,
        "total": tasks.len(),
    })))
}

pub async fn get_import(
    State(state): State<Arc<AppState>>,
    Extension(_auth): Extension<AuthInfo>,
    Path(id): Path<String>,
) -> Result<Json<ImportTaskRecord>, OmemError> {
    state
        .space_store
        .get_import_task(&id)
        .await?
        .map(Json)
        .ok_or_else(|| OmemError::NotFound(format!("import task {id}")))
}

pub async fn trigger_intelligence(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Path(id): Path<String>,
) -> Result<Json<ImportTaskRecord>, OmemError> {
    let mut task = state
        .space_store
        .get_import_task(&id)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("import task {id}")))?;

    if task.status == "processing" {
        return Err(OmemError::Validation(
            "intelligence task already running".to_string(),
        ));
    }

    task.status = "processing".to_string();
    task.extraction_status = "pending".to_string();
    task.reconcile_status = "pending".to_string();
    task.completed_at = None;
    state.space_store.update_import_task(&task).await?;

    let store = state.store_manager.get_store(&task.space_id).await?;

    let session_uri = format!("{}/{}", state.config.store_uri(), task.space_id);
    let session_store = Arc::new(
        SessionStore::new(&session_uri)
            .await
            .map_err(|e| OmemError::Storage(format!("session store: {e}")))?,
    );
    session_store.init_table().await?;

    let bg_embed = state.embed.clone();
    let bg_llm = state.llm.clone();
    let bg_space_store = state.space_store.clone();
    let bg_task_id = task.id.clone();
    let bg_tenant_id = auth.tenant_id.clone();
    let bg_strategy = task.strategy.clone();
    let sem = state.import_semaphore.clone();
    let reconcile_sem = state.reconcile_semaphore.clone();

    tokio::spawn(async move {
        let _permit = sem.acquire_owned().await.expect("semaphore");
        let intelligence = IntelligenceTask::new(
            store,
            session_store,
            bg_embed,
            bg_llm,
            bg_space_store,
            reconcile_sem,
            bg_task_id.clone(),
            bg_tenant_id,
            bg_strategy,
        );
        intelligence.run().await;
    });

    Ok(Json(task))
}

pub async fn cross_reconcile(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
) -> Result<Json<serde_json::Value>, OmemError> {
    let space_id = personal_space_id(&auth.tenant_id);
    let store = state.store_manager.get_store(&space_id).await?;

    let all_memories = store.list_all_active().await?;
    let mut relations_created = 0usize;
    let scanned = all_memories.len();

    for memory in &all_memories {
        let text = if memory.l0_abstract.is_empty() {
            &memory.content
        } else {
            &memory.l0_abstract
        };
        if text.is_empty() {
            continue;
        }

        let texts = vec![text.clone()];
        let embeddings = state.embed.embed(&texts).await?;
        let query_vec = match embeddings.first() {
            Some(v) => v,
            None => continue,
        };

        // limit=6 to account for self appearing in results
        let similar = store.vector_search(query_vec, 6, 0.85, None, None).await?;

        for (candidate, score) in &similar {
            if candidate.id == memory.id {
                continue;
            }

            // Re-fetch to see relations added in prior iterations of this loop
            let mut src = store
                .get_by_id(&memory.id)
                .await?
                .ok_or_else(|| OmemError::NotFound(memory.id.clone()))?;

            if src.relations.iter().any(|r| r.target_id == candidate.id) {
                continue;
            }

            src.relations.push(MemoryRelation {
                relation_type: RelationType::Supports,
                target_id: candidate.id.clone(),
                context_label: Some(format!("similarity:{:.2}", score)),
            });
            src.updated_at = chrono::Utc::now().to_rfc3339();
            store.update(&src, None).await?;

            relations_created += 1;
        }
    }

    Ok(Json(serde_json::json!({
        "relations_created": relations_created,
        "memories_scanned": scanned,
    })))
}

fn sha256_hex(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    hasher
        .finalize()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

pub async fn rollback_import(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    Path(id): Path<String>,
) -> Result<Json<serde_json::Value>, OmemError> {
    let task = state
        .space_store
        .get_import_task(&id)
        .await?
        .ok_or_else(|| OmemError::NotFound(format!("import task {id}")))?;

    let session_uri = format!("{}/{}", state.config.store_uri(), task.space_id);
    let session_store = SessionStore::new(&session_uri)
        .await
        .map_err(|e| OmemError::Storage(format!("session store: {e}")))?;
    session_store.init_table().await?;
    let sessions_deleted = session_store
        .delete_by_session_id(&format!("import-{id}"))
        .await?;

    let store = state
        .store_manager
        .get_store(&personal_space_id(&auth.tenant_id))
        .await?;
    let filter = format!(
        "source = 'intelligence' AND created_at >= '{}'",
        task.created_at.replace('\'', "''")
    );
    let memories_deleted = store.batch_soft_delete(&filter).await?;

    let mut updated_task = task;
    updated_task.status = "rolled_back".to_string();
    state.space_store.update_import_task(&updated_task).await?;

    Ok(Json(serde_json::json!({
        "deleted_memories": memories_deleted,
        "deleted_sessions": sessions_deleted,
        "import_status": "rolled_back"
    })))
}
