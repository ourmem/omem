use thiserror::Error;

#[derive(Error, Debug)]
pub enum OmemError {
    #[error("not found: {0}")]
    NotFound(String),

    #[error("unauthorized: {0}")]
    Unauthorized(String),

    #[error("validation error: {0}")]
    Validation(String),

    #[error("storage error: {0}")]
    Storage(String),

    #[error("embedding error: {0}")]
    Embedding(String),

    #[error("llm error: {0}")]
    Llm(String),

    #[error("rate limited")]
    RateLimited,

    #[error("internal error: {0}")]
    Internal(String),

    #[error("content too long: {length} chars exceed the embedder's context window")]
    ContentTooLong { length: usize, hint: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_display_messages() {
        assert_eq!(
            OmemError::NotFound("memory xyz".into()).to_string(),
            "not found: memory xyz"
        );
        assert_eq!(
            OmemError::Unauthorized("bad token".into()).to_string(),
            "unauthorized: bad token"
        );
        assert_eq!(
            OmemError::Validation("empty content".into()).to_string(),
            "validation error: empty content"
        );
        assert_eq!(
            OmemError::Storage("connection lost".into()).to_string(),
            "storage error: connection lost"
        );
        assert_eq!(
            OmemError::Embedding("model unavailable".into()).to_string(),
            "embedding error: model unavailable"
        );
        assert_eq!(
            OmemError::Llm("timeout".into()).to_string(),
            "llm error: timeout"
        );
        assert_eq!(OmemError::RateLimited.to_string(), "rate limited");
        assert_eq!(
            OmemError::Internal("panic".into()).to_string(),
            "internal error: panic"
        );
    }

    #[test]
    fn error_is_std_error() {
        let err: Box<dyn std::error::Error> = Box::new(OmemError::NotFound("test".into()));
        assert!(err.to_string().contains("not found"));
    }
}
