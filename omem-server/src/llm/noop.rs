use crate::domain::error::OmemError;
use crate::llm::service::LlmService;

pub struct NoopLlm;

#[async_trait::async_trait]
impl LlmService for NoopLlm {
    async fn complete_text(&self, _system: &str, _user: &str) -> Result<String, OmemError> {
        Err(OmemError::Llm("LLM not configured".to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn noop_returns_error() {
        let llm = NoopLlm;
        let result = llm.complete_text("sys", "user").await;
        assert!(result.is_err());
        assert!(result
            .err()
            .unwrap()
            .to_string()
            .contains("LLM not configured"));
    }
}
