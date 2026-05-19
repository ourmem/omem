use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION, CONTENT_TYPE};
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::config::OmemConfig;
use crate::domain::error::OmemError;
use crate::llm::service::LlmService;

const MAX_RETRIES: u32 = 3;
const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Serialize)]
struct ChatRequest {
    model: String,
    messages: Vec<ChatMessage>,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    response_format: Option<ResponseFormat>,
}

#[derive(Serialize, Clone)]
struct ChatMessage {
    role: String,
    content: String,
}

#[derive(Serialize)]
struct ResponseFormat {
    #[serde(rename = "type")]
    format_type: String,
}

#[derive(Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Deserialize)]
struct ChatChoice {
    message: ChatChoiceMessage,
}

#[derive(Deserialize)]
struct ChatChoiceMessage {
    content: Option<String>,
}

pub struct OpenAICompatLlm {
    client: reqwest::Client,
    url: String,
    model: String,
}

impl OpenAICompatLlm {
    pub fn new(config: &OmemConfig) -> Result<Self, OmemError> {
        let base_url = config.llm_base_url.trim_end_matches('/');
        if base_url.is_empty() {
            return Err(OmemError::Llm(
                "llm_base_url is required for openai-compatible provider".to_string(),
            ));
        }

        let mut headers = HeaderMap::new();
        headers.insert(CONTENT_TYPE, HeaderValue::from_static("application/json"));
        if !config.llm_api_key.is_empty() {
            let auth_value = format!("Bearer {}", config.llm_api_key);
            headers.insert(
                AUTHORIZATION,
                HeaderValue::from_str(&auth_value)
                    .map_err(|e| OmemError::Llm(format!("invalid api key header: {e}")))?,
            );
        }

        let client = reqwest::Client::builder()
            .timeout(TIMEOUT)
            .default_headers(headers)
            .build()
            .map_err(|e| OmemError::Llm(format!("failed to build http client: {e}")))?;

        Ok(Self {
            client,
            url: format!("{base_url}/v1/chat/completions"),
            model: config.llm_model.clone(),
        })
    }

    fn build_request(&self, system: &str, user: &str) -> ChatRequest {
        ChatRequest {
            model: self.model.clone(),
            messages: vec![
                ChatMessage {
                    role: "system".to_string(),
                    content: system.to_string(),
                },
                ChatMessage {
                    role: "user".to_string(),
                    content: user.to_string(),
                },
            ],
            temperature: 0.1,
            response_format: Some(ResponseFormat {
                format_type: "json_object".to_string(),
            }),
        }
    }
}

#[async_trait::async_trait]
impl LlmService for OpenAICompatLlm {
    async fn complete_text(&self, system: &str, user: &str) -> Result<String, OmemError> {
        let request = self.build_request(system, user);

        let mut last_err = None;
        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                let backoff = Duration::from_millis(500 * 2_u64.pow(attempt));
                tokio::time::sleep(backoff).await;
            }

            let resp = self
                .client
                .post(&self.url)
                .json(&request)
                .send()
                .await
                .map_err(|e| OmemError::Llm(format!("request failed: {e}")))?;

            let status = resp.status();
            if status == reqwest::StatusCode::TOO_MANY_REQUESTS && attempt + 1 < MAX_RETRIES {
                last_err = Some("rate limited (429)".to_string());
                continue;
            }

            if !status.is_success() {
                let body = resp.text().await.unwrap_or_default();
                return Err(OmemError::Llm(format!("LLM API returned {status}: {body}")));
            }

            let body: ChatResponse = resp
                .json()
                .await
                .map_err(|e| OmemError::Llm(format!("failed to parse response: {e}")))?;

            let content = body
                .choices
                .into_iter()
                .next()
                .and_then(|c| c.message.content)
                .unwrap_or_default();

            return Ok(content);
        }

        Err(OmemError::Llm(
            last_err.unwrap_or_else(|| "max retries exceeded".to_string()),
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_fails_without_base_url() {
        let config = OmemConfig {
            llm_base_url: String::new(),
            llm_provider: "openai-compatible".to_string(),
            ..OmemConfig::default()
        };
        let err = OpenAICompatLlm::new(&config).err().expect("should fail");
        assert!(err.to_string().contains("llm_base_url is required"));
    }

    #[test]
    fn new_succeeds_with_defaults() {
        let config = OmemConfig::default();
        let llm = OpenAICompatLlm::new(&config).unwrap();
        assert_eq!(llm.url, "https://api.openai.com/v1/chat/completions");
        assert_eq!(llm.model, "gpt-4o-mini");
    }

    #[test]
    fn trailing_slash_stripped() {
        let config = OmemConfig {
            llm_base_url: "http://localhost:8000/".to_string(),
            ..OmemConfig::default()
        };
        let llm = OpenAICompatLlm::new(&config).unwrap();
        assert_eq!(llm.url, "http://localhost:8000/v1/chat/completions");
    }

    #[test]
    fn build_request_structure() {
        let config = OmemConfig::default();
        let llm = OpenAICompatLlm::new(&config).unwrap();
        let req = llm.build_request("sys prompt", "user prompt");

        assert_eq!(req.model, "gpt-4o-mini");
        assert_eq!(req.messages.len(), 2);
        assert_eq!(req.messages[0].role, "system");
        assert_eq!(req.messages[0].content, "sys prompt");
        assert_eq!(req.messages[1].role, "user");
        assert_eq!(req.messages[1].content, "user prompt");
        assert!((req.temperature - 0.1).abs() < f32::EPSILON);
        assert_eq!(
            req.response_format.as_ref().map(|f| f.format_type.as_str()),
            Some("json_object")
        );
    }

    #[test]
    fn request_serialization() {
        let config = OmemConfig::default();
        let llm = OpenAICompatLlm::new(&config).unwrap();
        let req = llm.build_request("be helpful", "hello");

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["model"], "gpt-4o-mini");
        assert_eq!(json["messages"][0]["role"], "system");
        assert_eq!(json["messages"][1]["content"], "hello");
        assert_eq!(json["response_format"]["type"], "json_object");
    }
}
