//! Windows live-capture routes.
//!
//! Fixtures stand in for WASAPI loopback. Each channel keeps its own transcript.

use axum::extract::{Path, State};
use axum::{Json, Router};
use serde_json::json;
use talkcer_core::{CaptureAction, CaptureFixture, CaptureStatus, TranscriptUpsert};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use crate::ws::publish_transcript;

/// Capture routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/sessions/{id}/capture", axum::routing::get(status).post(control))
        .route("/api/sessions/{id}/capture/fixture", axum::routing::post(inject))
}

#[derive(serde::Deserialize)]
struct ControlBody {
    action: CaptureAction,
}

async fn status(State(state): State<AppState>, Path(id): Path<Uuid>) -> Result<Json<CaptureStatus>, ApiError> {
    Ok(Json(state.capture.status(id).await?))
}

async fn control(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<ControlBody>,
) -> Result<Json<CaptureStatus>, ApiError> {
    let status = state.capture.control(id, body.action).await?;
    tracing::info!(session_id = %id, state = ?status.state, platform = status.platform, "capture controlled");
    Ok(Json(status))
}

async fn inject(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(fixture): Json<CaptureFixture>,
) -> Result<Json<TranscriptUpsert>, ApiError> {
    let saved = state.capture.inject(id, fixture).await?;
    publish_transcript(&state, &saved.item);
    tracing::info!(session_id = %id, utterance_id = %saved.item.id, channel = %json!(saved.item.metadata["audio_channel"]), "capture fixture stored");
    Ok(Json(saved))
}
