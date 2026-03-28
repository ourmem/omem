use std::sync::Arc;

use axum::extract::{Extension, State};
use axum_extra::extract::Multipart;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use crate::api::server::{AppState, personal_space_id};
use crate::domain::category::Category;
use crate::domain::error::OmemError;
use crate::domain::memory::Memory;
use crate::domain::tenant::AuthInfo;
use crate::domain::types::MemoryType;
use crate::multimodal::MultiModalService;

const MAX_FILE_SIZE: usize = 50 * 1024 * 1024;

#[derive(Serialize)]
pub struct UploadResponse {
    pub task_id: String,
    pub filename: String,
    pub content_type: String,
    pub chunks_created: usize,
}

pub async fn upload_file(
    State(state): State<Arc<AppState>>,
    Extension(auth): Extension<AuthInfo>,
    mut multipart: Multipart,
) -> Result<impl IntoResponse, OmemError> {
    let mut file_data: Option<(String, String, Vec<u8>)> = None;

    while let Some(field) = multipart
        .next_field()
        .await
        .map_err(|e| OmemError::Validation(format!("multipart error: {e}")))?
    {
        let name = field.name().unwrap_or("").to_string();
        if name == "file" {
            let filename = field
                .file_name()
                .unwrap_or("unknown")
                .to_string();
            let content_type = field
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string();
            let data = field
                .bytes()
                .await
                .map_err(|e| OmemError::Validation(format!("failed to read file: {e}")))?;

            if data.len() > MAX_FILE_SIZE {
                return Err(OmemError::Validation(format!(
                    "file too large: {} bytes (max {})",
                    data.len(),
                    MAX_FILE_SIZE
                )));
            }

            file_data = Some((filename, content_type, data.to_vec()));
            break;
        }
    }

    let (filename, mime, data) = file_data
        .ok_or_else(|| OmemError::Validation("no 'file' field in multipart body".to_string()))?;

    let store = state.store_manager.get_store(&personal_space_id(&auth.tenant_id)).await?;

    let detected = MultiModalService::detect_content_type(&filename, &mime);
    let content_type_str = format!("{detected:?}");

    let chunks = MultiModalService::process_file(&data, &detected, &filename)?;

    let task_id = uuid::Uuid::new_v4().to_string();
    let chunk_count = chunks.len();

    let embed = state.embed.clone();
    let tenant_id = auth.tenant_id.clone();
    let agent_id = auth.agent_id.clone();
    let fname = filename.clone();

    tokio::spawn(async move {
        for chunk in chunks {
            let mut memory = Memory::new(
                &chunk.content,
                Category::Entities,
                MemoryType::Session,
                &tenant_id,
            );
            memory.tags = vec![
                format!("file:{fname}"),
                format!("chunk_type:{}", chunk.chunk_type),
            ];
            memory.source = Some(format!("file_upload:{fname}"));
            memory.agent_id = agent_id.clone();

            let vectors = match embed.embed(std::slice::from_ref(&chunk.content)).await {
                Ok(v) => v.into_iter().next(),
                Err(e) => {
                    tracing::error!(error = %e, "failed to embed file chunk");
                    None
                }
            };

            if let Err(e) = store.create(&memory, vectors.as_deref()).await {
                tracing::error!(error = %e, "failed to store file chunk");
            }
        }
    });

    Ok((
        StatusCode::ACCEPTED,
        Json(UploadResponse {
            task_id,
            filename,
            content_type: content_type_str,
            chunks_created: chunk_count,
        }),
    ))
}
