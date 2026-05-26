//! API error type implementing `IntoResponse` so handlers can return `Result<T, ApiError>`.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub message: String,
}

impl ApiError {
    pub fn new(status: StatusCode, message: impl Into<String>) -> Self {
        Self {
            status,
            message: message.into(),
        }
    }

    pub fn not_found(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::NOT_FOUND, msg)
    }

    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::BAD_REQUEST, msg)
    }

    pub fn internal(msg: impl Into<String>) -> Self {
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, msg)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(json!({ "error": self.message }))).into_response()
    }
}

impl From<mnemos_core::error::MnemosError> for ApiError {
    fn from(e: mnemos_core::error::MnemosError) -> Self {
        use mnemos_core::error::MnemosError::*;
        match e {
            MemoryNotFound(_) | EntityNotFound(_) | SessionNotFound(_) => {
                Self::not_found(e.to_string())
            }
            Validation(_) | InvalidFrontmatter { .. } | MalformedFile { .. } => {
                Self::bad_request(e.to_string())
            }
            _ => Self::internal(e.to_string()),
        }
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> Self {
        Self::internal(e.to_string())
    }
}
