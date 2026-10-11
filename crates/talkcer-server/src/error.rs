//! HTTP mapping for core errors.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::json;
use talkcer_core::CoreError;

/// Error type returned by handlers.
#[derive(Debug)]
pub struct ApiError(pub CoreError);

impl From<CoreError> for ApiError {
    fn from(error: CoreError) -> Self {
        Self(error)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            CoreError::InvalidMode(_)
            | CoreError::InvalidStatus(_)
            | CoreError::InvalidTransition { .. }
            | CoreError::EmptyTitle
            | CoreError::GroupedFilterConflict => {
                StatusCode::BAD_REQUEST
            }
            CoreError::NotFound => StatusCode::NOT_FOUND,
            CoreError::Database(_) | CoreError::Migration(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = Json(json!({
            "error": self.0.to_string(),
        }));
        (status, body).into_response()
    }
}
