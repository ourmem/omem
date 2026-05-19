use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::config::OmemConfig;
use crate::domain::error::OmemError;
use crate::embed::service::EmbedService;

const MAX_BATCH_SIZE: usize = 25;
const MAX_RETRIES: u32 = 3;
const DEFAULT_TIMEOUT_SECS: u64 = 10;

#[derive(Serialize)]
struct EmbeddingRequest {
    model: String,
    input: Vec<String>,
}

#[derive(Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
}

#[derive(Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

pub struct OpenAICompatEmbedder {
    client: reqwest::Client,
    url: String,
    model: String,
    dims: usize,
}

impl OpenAICompatEmbedder {
    pub fn new(config: &OmemConfig) -> Result<Self, OmemError> {
        let base_url = config.embed_base_url.trim_end_matches('/');
        if base_url.is_empty() {
            return Err(OmemError::Embedding(
                "embed_base_url is required for openai-compatible provider".to_string(),
            ));
        }

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if !config.embed_api_key.is_empty() {
            let auth_value = format!("Bearer {}", config.embed_api_key);
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&auth_value)
                    .map_err(|e| OmemError::Embedding(format!("invalid api key header: {e}")))?,
            );
        }

        let timeout_secs = if config.embed_timeout_secs > 0 {
            config.embed_timeout_secs
        } else {
            DEFAULT_TIMEOUT_SECS
        };
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(timeout_secs))
            .default_headers(headers)
            .build()
            .map_err(|e| OmemError::Embedding(format!("failed to build http client: {e}")))?;

        Ok(Self {
            client,
            url: format!("{base_url}/v1/embeddings"),
            model: config.embed_model.clone(),
            dims: if config.embed_dim > 0 {
                config.embed_dim
            } else {
                1024
            },
        })
    }

    async fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, OmemError> {
        let request = EmbeddingRequest {
            model: self.model.clone(),
            input: texts,
        };

        let mut last_err = None;
        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                let backoff = Duration::from_millis(100 * 2_u64.pow(attempt));
                tokio::time::sleep(backoff).await;
            }

            let resp = self
                .client
                .post(&self.url)
                .json(&request)
                .send()
                .await
                .map_err(|e| OmemError::Embedding(format!("request failed: {e}")))?;

            let status = resp.status();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS && attempt + 1 < MAX_RETRIES {
                last_err = Some("rate limited (429)".to_string());
                continue;
            }

            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                return Err(OmemError::Embedding(format!(
                    "embedding API returned {status}: {body}"
                )));
            }

            let body: EmbeddingResponse = resp
                .json()
                .await
                .map_err(|e| OmemError::Embedding(format!("failed to parse response: {e}")))?;

            return Ok(body.data.into_iter().map(|d| d.embedding).collect());
        }

        Err(OmemError::Embedding(
            last_err.unwrap_or_else(|| "max retries exceeded".to_string()),
        ))
    }
}

#[async_trait::async_trait]
impl EmbedService for OpenAICompatEmbedder {
    async fn embed(&self, texts: &[String]) -> Result<Vec<Vec<f32>>, OmemError> {
        if texts.is_empty() {
            return Ok(Vec::new());
        }

        let mut all_embeddings = Vec::with_capacity(texts.len());
        for chunk in texts.chunks(MAX_BATCH_SIZE) {
            let batch_result = self.embed_batch(chunk.to_vec()).await?;
            all_embeddings.extend(batch_result);
        }
        Ok(all_embeddings)
    }

    fn dimensions(&self) -> usize {
        self.dims
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_fails_without_base_url() {
        let config = OmemConfig {
            embed_base_url: String::new(),
            ..OmemConfig::default()
        };
        let err = OpenAICompatEmbedder::new(&config)
            .err()
            .expect("should fail");
        assert!(err.to_string().contains("embed_base_url is required"));
    }

    #[test]
    fn new_succeeds_with_base_url() {
        let config = OmemConfig {
            embed_base_url: "http://localhost:8000".to_string(),
            embed_model: "text-embedding-3-small".to_string(),
            ..OmemConfig::default()
        };
        let embedder = OpenAICompatEmbedder::new(&config).unwrap();
        assert_eq!(embedder.url, "http://localhost:8000/v1/embeddings");
        assert_eq!(embedder.model, "text-embedding-3-small");
        assert_eq!(embedder.dimensions(), 1024);
    }

    #[test]
    fn trailing_slash_stripped() {
        let config = OmemConfig {
            embed_base_url: "http://localhost:8000/".to_string(),
            ..OmemConfig::default()
        };
        let embedder = OpenAICompatEmbedder::new(&config).unwrap();
        assert_eq!(embedder.url, "http://localhost:8000/v1/embeddings");
    }

    #[test]
    fn embed_dim_from_config_overrides_default() {
        // 384 is a common smaller-model dim (all-MiniLM, bge-small).
        let config = OmemConfig {
            embed_base_url: "http://localhost:11434".to_string(),
            embed_dim: 384,
            ..OmemConfig::default()
        };
        let embedder = OpenAICompatEmbedder::new(&config).unwrap();
        assert_eq!(embedder.dimensions(), 384);
    }

    #[test]
    fn embed_dim_zero_falls_back_to_1024() {
        // Guards against misconfiguration that would produce zero-length vectors.
        let config = OmemConfig {
            embed_base_url: "http://localhost:11434".to_string(),
            embed_dim: 0,
            ..OmemConfig::default()
        };
        let embedder = OpenAICompatEmbedder::new(&config).unwrap();
        assert_eq!(embedder.dimensions(), 1024);
    }

    #[test]
    fn embed_timeout_zero_falls_back_to_default() {
        // Constructor must not pass a 0s timeout to reqwest (would block forever
        // or return immediately depending on version). Defaults to 10s when 0.
        let config = OmemConfig {
            embed_base_url: "http://localhost:11434".to_string(),
            embed_timeout_secs: 0,
            ..OmemConfig::default()
        };
        // Just assert construction succeeds — timeout itself is baked into the
        // reqwest client and not externally observable without sending traffic.
        OpenAICompatEmbedder::new(&config).expect("construction with timeout=0 should fall back");
    }
}
