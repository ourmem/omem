use std::time::Duration;

use reqwest::Client;
use serde::{Deserialize, Serialize};

use crate::domain::error::OmemError;

#[derive(Debug, Clone)]
pub struct Reranker {
    provider: String,
    endpoint: String,
    api_key: String,
    client: Client,
    timeout: Duration,
}

#[derive(Serialize)]
struct RerankRequest<'a> {
    query: &'a str,
    documents: Vec<&'a str>,
    top_n: usize,
}

#[derive(Deserialize)]
struct RerankResponse {
    results: Vec<RerankResult>,
}

#[derive(Deserialize)]
struct RerankResult {
    index: usize,
    relevance_score: f32,
}

impl Reranker {
    pub fn from_env() -> Option<Self> {
        let provider = std::env::var("OMEM_RERANK_PROVIDER").unwrap_or_default();
        if provider.is_empty() || provider == "none" {
            return None;
        }

        let api_key = std::env::var("OMEM_RERANK_API_KEY").unwrap_or_default();
        let endpoint =
            std::env::var("OMEM_RERANK_ENDPOINT").unwrap_or_else(|_| match provider.as_str() {
                "jina" => "https://api.jina.ai/v1/rerank".to_string(),
                "voyage" => "https://api.voyageai.com/v1/rerank".to_string(),
                "pinecone" => "https://api.pinecone.io/rerank".to_string(),
                _ => String::new(),
            });

        if endpoint.is_empty() {
            return None;
        }

        Some(Self {
            provider,
            endpoint,
            api_key,
            client: Client::new(),
            timeout: Duration::from_secs(5),
        })
    }

    #[cfg(test)]
    pub fn new_with_endpoint(provider: &str, endpoint: &str, api_key: &str) -> Self {
        Self {
            provider: provider.to_string(),
            endpoint: endpoint.to_string(),
            api_key: api_key.to_string(),
            client: Client::new(),
            timeout: Duration::from_secs(5),
        }
    }

    pub fn provider(&self) -> &str {
        &self.provider
    }

    pub async fn rerank(&self, query: &str, documents: &[&str]) -> Result<Vec<f32>, OmemError> {
        if documents.is_empty() {
            return Ok(Vec::new());
        }

        let body = RerankRequest {
            query,
            documents: documents.to_vec(),
            top_n: documents.len(),
        };

        let response = self
            .client
            .post(&self.endpoint)
            .bearer_auth(&self.api_key)
            .json(&body)
            .timeout(self.timeout)
            .send()
            .await
            .map_err(|e| OmemError::Internal(format!("rerank request failed: {e}")))?;

        if !response.status().is_success() {
            let status = response.status();
            let body_text = response
                .text()
                .await
                .unwrap_or_else(|_| "unknown".to_string());
            return Err(OmemError::Internal(format!(
                "rerank API returned {status}: {body_text}"
            )));
        }

        let rerank_resp: RerankResponse = response
            .json()
            .await
            .map_err(|e| OmemError::Internal(format!("rerank response parse failed: {e}")))?;

        let mut scores = vec![0.0f32; documents.len()];
        for result in rerank_resp.results {
            if result.index < scores.len() {
                scores[result.index] = result.relevance_score;
            }
        }

        Ok(scores)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_from_env_none_provider() {
        std::env::remove_var("OMEM_RERANK_PROVIDER");
        let reranker = Reranker::from_env();
        assert!(reranker.is_none());
    }

    #[test]
    fn test_from_env_explicit_none() {
        std::env::set_var("OMEM_RERANK_PROVIDER", "none");
        let reranker = Reranker::from_env();
        assert!(reranker.is_none());
        std::env::remove_var("OMEM_RERANK_PROVIDER");
    }

    #[test]
    fn test_new_with_endpoint() {
        let reranker = Reranker::new_with_endpoint("jina", "http://localhost:8080/rerank", "key");
        assert_eq!(reranker.provider(), "jina");
        assert_eq!(reranker.endpoint, "http://localhost:8080/rerank");
    }

    #[tokio::test]
    async fn test_rerank_empty_documents() {
        let reranker = Reranker::new_with_endpoint("jina", "http://localhost:1/rerank", "key");
        let result = reranker.rerank("query", &[]).await;
        assert!(result.is_ok());
        assert!(result.expect("should be ok").is_empty());
    }

    #[tokio::test]
    async fn test_rerank_timeout_returns_error() {
        let reranker = Reranker {
            provider: "jina".to_string(),
            endpoint: "http://192.0.2.1:1/rerank".to_string(),
            api_key: "test".to_string(),
            client: Client::new(),
            timeout: Duration::from_millis(100),
        };

        let result = reranker.rerank("query", &["doc1", "doc2"]).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_rerank_request_serialization() {
        let req = RerankRequest {
            query: "test query",
            documents: vec!["doc1", "doc2"],
            top_n: 2,
        };
        let json = serde_json::to_value(&req).expect("serialize");
        assert_eq!(json["query"], "test query");
        assert_eq!(json["documents"].as_array().expect("array").len(), 2);
        assert_eq!(json["top_n"], 2);
    }

    #[test]
    fn test_rerank_response_deserialization() {
        let json =
            r#"{"results":[{"index":1,"relevance_score":0.9},{"index":0,"relevance_score":0.5}]}"#;
        let resp: RerankResponse = serde_json::from_str(json).expect("deserialize");
        assert_eq!(resp.results.len(), 2);
        assert_eq!(resp.results[0].index, 1);
        assert!((resp.results[0].relevance_score - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn test_score_mapping_to_original_order() {
        let results = vec![
            RerankResult {
                index: 2,
                relevance_score: 0.9,
            },
            RerankResult {
                index: 0,
                relevance_score: 0.7,
            },
            RerankResult {
                index: 1,
                relevance_score: 0.3,
            },
        ];
        let mut scores = [0.0f32; 3];
        for r in results {
            if r.index < scores.len() {
                scores[r.index] = r.relevance_score;
            }
        }
        assert!((scores[0] - 0.7).abs() < f32::EPSILON);
        assert!((scores[1] - 0.3).abs() < f32::EPSILON);
        assert!((scores[2] - 0.9).abs() < f32::EPSILON);
    }
}
