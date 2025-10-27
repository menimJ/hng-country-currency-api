use axum::{http::StatusCode, response::{IntoResponse, Response}, Json};
use serde::Serialize;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ApiError {
    #[error("validation: {0}")]
    Validation(String),
    #[error("not_found: {0}")]
    NotFound(String),
    #[error("external_unavailable: {0}")]
    External(String),
    #[error("internal: {0}")]
    Internal(String),
}

#[derive(Serialize)]
pub struct ErrorBody<'a> {
    pub error: &'a str,
    #[serde(skip_serializing_if = "Option::is_none")] pub details: Option<String>,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            ApiError::Validation(msg) => (
                StatusCode::BAD_REQUEST,
                Json(ErrorBody { error: "Validation failed", details: Some(msg) }),
            ).into_response(),
            ApiError::NotFound(_) => (
                StatusCode::NOT_FOUND,
                Json(ErrorBody { error: "Country not found", details: None }),
            ).into_response(),
            ApiError::External(msg) => (
                StatusCode::SERVICE_UNAVAILABLE,
                Json(ErrorBody { error: "External data source unavailable", details: Some(msg) }),
            ).into_response(),
            ApiError::Internal(msg) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(ErrorBody { error: "Internal server error", details: Some(msg) }),
            ).into_response(),
        }
    }
}
