use aws_sdk_bedrockruntime::primitives::Blob;
use aws_sdk_bedrockruntime::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;

use crate::domain::error::OmemError;
use crate::llm::service::LlmService;

const DEFAULT_MODEL_ID: &str = "anthropic.claude-3-haiku-20240307-v1:0";
const MAX_RETRIES: u32 = 3;
const TIMEOUT: Duration = Duration::from_secs(30);

#[derive(Serialize)]
struct ClaudeRequest {
    anthropic_version: &'static str,
    max_tokens: u32,
    temperature: f32,
    #[serde(skip_serializing_if = "Option::is_none")]
    system: Option<String>,
    messages: Vec<ClaudeMessage>,
}

#[derive(Serialize)]
struct ClaudeMessage {
    role: &'static str,
    content: String,
}

#[derive(Deserialize)]
struct ClaudeResponse {
    content: Vec<ClaudeContent>,
}

#[derive(Deserialize)]
struct ClaudeContent {
    text: Option<String>,
}

pub struct BedrockLlm {
    client: Client,
    model_id: String,
}

impl BedrockLlm {
    pub async fn new(model_id: Option<&str>) -> Self {
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
            model_id: model_id.unwrap_or(DEFAULT_MODEL_ID).to_string(),
        }
    }
}

#[async_trait::async_trait]
impl LlmService for BedrockLlm {
    async fn complete_text(&self, system: &str, user: &str) -> Result<String, OmemError> {
        let request_body = serde_json::to_vec(&ClaudeRequest {
            anthropic_version: "bedrock-2023-05-31",
            max_tokens: 4096,
            temperature: 0.1,
            system: if system.is_empty() {
                None
            } else {
                Some(system.to_string())
            },
            messages: vec![ClaudeMessage {
                role: "user",
                content: user.to_string(),
            }],
        })
        .map_err(|e| OmemError::Llm(format!("failed to serialize request: {e}")))?;

        let mut last_err = None;
        for attempt in 0..MAX_RETRIES {
            if attempt > 0 {
                let backoff = Duration::from_millis(500 * 2_u64.pow(attempt));
                tokio::time::sleep(backoff).await;
            }

            match self
                .client
                .invoke_model()
                .model_id(&self.model_id)
                .content_type("application/json")
                .accept("application/json")
                .body(Blob::new(request_body.clone()))
                .send()
                .await
            {
                Ok(output) => {
                    let resp: ClaudeResponse = serde_json::from_slice(output.body().as_ref())
                        .map_err(|e| OmemError::Llm(format!("failed to parse response: {e}")))?;
                    let text = resp
                        .content
                        .into_iter()
                        .filter_map(|c| c.text)
                        .collect::<Vec<_>>()
                        .join("");
                    return Ok(text);
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
                    return Err(OmemError::Llm(format!("bedrock invoke_model failed: {e}")));
                }
            }
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
    fn claude_request_serialization() {
        let req = ClaudeRequest {
            anthropic_version: "bedrock-2023-05-31",
            max_tokens: 4096,
            temperature: 0.1,
            system: Some("be helpful".to_string()),
            messages: vec![ClaudeMessage {
                role: "user",
                content: "hello".to_string(),
            }],
        };

        let json = serde_json::to_value(&req).unwrap();
        assert_eq!(json["anthropic_version"], "bedrock-2023-05-31");
        assert_eq!(json["max_tokens"], 4096);
        assert_eq!(json["messages"][0]["role"], "user");
        assert_eq!(json["messages"][0]["content"], "hello");
        assert_eq!(json["system"], "be helpful");
    }

    #[test]
    fn claude_request_skips_empty_system() {
        let req = ClaudeRequest {
            anthropic_version: "bedrock-2023-05-31",
            max_tokens: 4096,
            temperature: 0.1,
            system: None,
            messages: vec![ClaudeMessage {
                role: "user",
                content: "test".to_string(),
            }],
        };

        let json = serde_json::to_value(&req).unwrap();
        assert!(json.get("system").is_none());
    }

    #[test]
    fn claude_response_deserialization() {
        let json = r#"{"content":[{"type":"text","text":"hello world"}],"stop_reason":"end_turn"}"#;
        let resp: ClaudeResponse = serde_json::from_str(json).unwrap();
        assert_eq!(resp.content.len(), 1);
        assert_eq!(resp.content[0].text.as_deref(), Some("hello world"));
    }
}
