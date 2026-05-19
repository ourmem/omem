#[cfg(feature = "bedrock")]
mod bedrock;
mod noop;
mod openai_compat;
mod service;

#[cfg(feature = "bedrock")]
pub use bedrock::BedrockLlm;
pub use noop::NoopLlm;
pub use openai_compat::OpenAICompatLlm;
pub use service::{complete_json, strip_markdown_fences, LlmService};

use crate::config::OmemConfig;
use crate::domain::error::OmemError;

pub async fn create_llm_service(config: &OmemConfig) -> Result<Box<dyn LlmService>, OmemError> {
    match config.llm_provider.as_str() {
        "openai-compatible" => Ok(Box::new(OpenAICompatLlm::new(config)?)),
        #[cfg(feature = "bedrock")]
        "bedrock" => {
            let model = if config.llm_model.is_empty() || config.llm_model == "gpt-4o-mini" {
                None
            } else {
                Some(config.llm_model.as_str())
            };
            Ok(Box::new(BedrockLlm::new(model).await))
        }
        #[cfg(not(feature = "bedrock"))]
        "bedrock" => Err(OmemError::Llm(
            "bedrock feature not enabled (musl build)".to_string(),
        )),
        _ => Ok(Box::new(NoopLlm)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn factory_returns_noop_by_default() {
        let config = OmemConfig::default();
        let svc = create_llm_service(&config).await.unwrap();
        let result = svc.complete_text("sys", "user").await;
        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("LLM not configured"));
    }

    #[tokio::test]
    async fn factory_returns_noop_for_unknown_provider() {
        let config = OmemConfig {
            llm_provider: "unknown-provider".to_string(),
            ..OmemConfig::default()
        };
        let svc = create_llm_service(&config).await.unwrap();
        let result = svc.complete_text("sys", "user").await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn factory_openai_compat_succeeds_with_defaults() {
        let config = OmemConfig {
            llm_provider: "openai-compatible".to_string(),
            ..OmemConfig::default()
        };
        let _svc = create_llm_service(&config).await.unwrap();
    }

    #[tokio::test]
    async fn factory_openai_compat_fails_without_base_url() {
        let config = OmemConfig {
            llm_provider: "openai-compatible".to_string(),
            llm_base_url: String::new(),
            ..OmemConfig::default()
        };
        let result = create_llm_service(&config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn complete_json_with_noop_returns_error() {
        let llm = NoopLlm;
        let result: Result<serde_json::Value, _> = complete_json(&llm, "sys", "user").await;
        assert!(result.is_err());
    }
}
