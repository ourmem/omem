use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

use crate::domain::error::OmemError;

impl IntoResponse for OmemError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            OmemError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            OmemError::Unauthorized(_) => (StatusCode::UNAUTHORIZED, "unauthorized"),
            OmemError::Validation(_) => (StatusCode::BAD_REQUEST, "validation_error"),
            OmemError::RateLimited => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            OmemError::Storage(_)
            | OmemError::Embedding(_)
            | OmemError::Llm(_)
            | OmemError::Internal(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };

        let message = self.to_string();
        tracing::error!(status = %status, code = code, error = %message, "request error");

        let body = json!({
            "error": {
                "code": code,
                "message": message,
            }
        });

        (status, Json(body)).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    #[tokio::test]
    async fn test_not_found_response() {
        let err = OmemError::NotFound("memory xyz".into());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let body = response.into_body();
        let bytes = body.collect().await.expect("collect body").to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&bytes).expect("parse json");
        assert_eq!(json["error"]["code"], "not_found");
    }

    #[tokio::test]
    async fn test_unauthorized_response() {
        let err = OmemError::Unauthorized("bad key".into());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_validation_response() {
        let err = OmemError::Validation("empty content".into());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn test_rate_limited_response() {
        let err = OmemError::RateLimited;
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[tokio::test]
    async fn test_internal_error_response() {
        let err = OmemError::Storage("db down".into());
        let response = err.into_response();
        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
