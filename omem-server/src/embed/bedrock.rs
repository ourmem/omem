use aws_sdk_bedrockruntime::primitives::Blob;
use aws_sdk_bedrockruntime::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::domain::error::OmemError;
use crate::embed::service::EmbedService;

const MODEL_ID: &str = "amazon.titan-embed-text-v2:0";
const DIMENSIONS: usize = 1024;
const MAX_RETRIES: u32 = 3;
const TIMEOUT: Duration = Duration::from_secs(10);

#[derive(Serialize)]
struct TitanEmbedRequest {
    #[serde(rename = "inputText")]
    input_text: String,
    dimensions: u32,
    normalize: bool,
}

#[derive(Deserialize)]
struct TitanEmbedResponse {
    embedding: Vec<f32>,
}

pub struct BedrockEmbedder {
    client: Client,
}

impl BedrockEmbedder {
    pub async fn new() -> Self {
        let sdk_config = aws_config::defaults(aws_config::BehaviorVersion::latest())
            .timeout_config(
                aws_sdk_bedrockruntime::config::timeout::TimeoutConfig::builder()
                    .operation_timeout(TIMEOUT)
                    .build(),
            )
            .load()
            .await;
        Self {
            client: Client::new(&sdk_config),
        }
    }

    async fn embed_single(&self, text: &str) -> Result<Vec<f32>, OmemError> {
        let request_body = serde_json::to_vec(&TitanEmbedRequest {
            input_text: text.to_string(),
            dimensions: DIMENSIONS as u32,
            normalize: true,
        })
        .map_err(|e| OmemError::Embedding(format!("failed to serialize request: {e}")))?;

        let mut last_err = None;
        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                let backoff = Duration::from_millis(100 * 2_u64.pow(attempt));
                tokio::time::sleep(backoff).await;
            }

            match self
                .client
                .invoke_model()
                .model_id(MODEL_ID)
                .content_type("application/json")
                .accept("application/json")
                .body(Blob::new(request_body.clone()))
                .send()
                .await
            {
                Ok(output) => {
                    let resp: TitanEmbedResponse = serde_json::from_slice(output.body().as_ref())
                        .map_err(|e| {
                        OmemError::Embedding(format!("failed to parse response: {e}"))
                    })?;
                    return Ok(resp.embedding);
                }
                Err(e) => {
                    let is_throttled = e
                        .as_service_error()
                        .map(|se| se.is_throttling_exception())
                        .unwrap_or(false);
                    if is_throttled && attempt + 1 < MAX_RETRIES {
                        last_err = Some(format!("throttled: {e}"));
                        continue;
                    }
                    return Err(OmemError::Embedding(format!(
                        "bedrock invoke_model failed: {e}"
                    )));
                }
            }
        }

        Err(OmemError::Embedding(
            last_err.unwrap_or_else(|| "max retries exceeded".to_string()),
        ))
    }
}

#[async_trait::async_trait]
impl EmbedService for BedrockEmbedder {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, OmemError> {
        let mut results = Vec::with_capacity(texts.len());
        for text in texts {
            results.push(self.embed_single(text).await?);
        }
        Ok(results)
    }

    fn dimensions(&self) -> usize {
        DIMENSIONS
    }
}
