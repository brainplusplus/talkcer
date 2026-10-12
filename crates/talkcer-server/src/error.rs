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
            | CoreError::EmptyText
            | CoreError::GroupedFilterConflict
            | CoreError::InvalidSpeaker(_)
            | CoreError::InvalidSource(_)
            | CoreError::InvalidItemKind(_)
            | CoreError::InvalidPurpose(_)
            | CoreError::InvalidAction(_)
            | CoreError::InvalidTarget
            | CoreError::InvalidTimestamp
            | CoreError::InvalidLimit
            | CoreError::InvalidProviderKind(_)
            | CoreError::InvalidConfiguration(_)
            | CoreError::UnknownProvider(_)
            | CoreError::InvalidCapability(_)
            | CoreError::EmptyName
            | CoreError::InvalidMockAction(_)
            | CoreError::InvalidLearningAction(_)
            | CoreError::InvalidAudio(_)
            | CoreError::InvalidCapture(_)
            | CoreError::InvalidOverlay(_)
            | CoreError::InvalidExport(_) => StatusCode::BAD_REQUEST,
            CoreError::AudioDevice(_)
            | CoreError::Transcription(_)
            | CoreError::ProviderUnavailable(_)
            | CoreError::Screen(_) => StatusCode::SERVICE_UNAVAILABLE,
            CoreError::Unauthorized => StatusCode::UNAUTHORIZED,
            CoreError::RateLimited => StatusCode::TOO_MANY_REQUESTS,
            CoreError::ProviderTimeout => StatusCode::GATEWAY_TIMEOUT,
            CoreError::UtteranceClosed | CoreError::UtteranceSessionMismatch => StatusCode::CONFLICT,
            CoreError::NotFound | CoreError::ItemNotFound => StatusCode::NOT_FOUND,
            CoreError::Database(_) | CoreError::Migration(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = Json(json!({
            "error": self.0.to_string(),
        }));
        (status, body).into_response()
    }
}
