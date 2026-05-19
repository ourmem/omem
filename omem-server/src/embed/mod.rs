#[cfg(feature = "bedrock")]
mod bedrock;
mod noop;
mod openai_compat;
mod service;

#[cfg(feature = "bedrock")]
pub use bedrock::BedrockEmbedder;
pub use noop::NoopEmbedder;
pub use openai_compat::OpenAICompatEmbedder;
pub use service::EmbedService;

use crate::config::OmemConfig;
use crate::domain::error::OmemError;

pub async fn create_embed_service(config: &OmemConfig) -> Result<Box<dyn EmbedService>, OmemError> {
    match config.embed_provider.as_str() {
        #[cfg(feature = "bedrock")]
        "bedrock" => Ok(Box::new(BedrockEmbedder::new().await)),
        #[cfg(not(feature = "bedrock"))]
        "bedrock" => Err(OmemError::Embedding(
            "bedrock feature not enabled (musl build)".to_string(),
        )),
        "openai-compatible" => Ok(Box::new(OpenAICompatEmbedder::new(config)?)),
        _ => Ok(Box::new(NoopEmbedder::new(1024))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn factory_returns_noop_by_default() {
        let config = OmemConfig::default();
        let svc = create_embed_service(&config).await.unwrap();
        assert_eq!(svc.dimensions(), 1024);

        let result = svc.embed(&["test".to_string()]).await.unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].len(), 1024);
        assert!(result[0].iter().all(|&v| v == 0.0));
    }

    #[tokio::test]
    async fn factory_returns_noop_for_unknown_provider() {
        let config = OmemConfig {
            embed_provider: "unknown-provider".to_string(),
            ..OmemConfig::default()
        };
        let svc = create_embed_service(&config).await.unwrap();
        assert_eq!(svc.dimensions(), 1024);
    }

    #[tokio::test]
    async fn factory_openai_compat_fails_without_base_url() {
        let config = OmemConfig {
            embed_provider: "openai-compatible".to_string(),
            embed_base_url: String::new(),
            ..OmemConfig::default()
        };
        let result = create_embed_service(&config).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn factory_openai_compat_succeeds_with_config() {
        let config = OmemConfig {
            embed_provider: "openai-compatible".to_string(),
            embed_base_url: "http://localhost:11434".to_string(),
            embed_model: "nomic-embed-text".to_string(),
            ..OmemConfig::default()
        };
        let svc = create_embed_service(&config).await.unwrap();
        assert_eq!(svc.dimensions(), 1024);
    }
}
