//! Windows live-capture routes.
//!
//! `start` opens the native WASAPI endpoints. Fixtures stand in for transcribed
//! audio, and each channel keeps its own transcript.

use axum::extract::{Path, State};
use axum::{Json, Router};
use serde_json::json;
use talkcer_core::{CaptureAction, CaptureDevice, CaptureFixture, CaptureStatus, TranscriptUpsert};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use crate::ws::publish_transcript;

/// Capture routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/sessions/{id}/capture", axum::routing::get(status).post(control))
        .route("/api/sessions/{id}/capture/fixture", axum::routing::post(inject))
        .route("/api/sessions/{id}/capture/devices", axum::routing::get(devices))
        .route("/api/devices/audio", axum::routing::get(audio_devices))
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
    tracing::info!(session_id = %id, state = ?status.state, platform = status.platform, devices = status.devices.len(), "capture controlled");
    Ok(Json(status))
}

/// `GET /api/devices/audio`: the default endpoints on this computer.
async fn audio_devices(State(state): State<AppState>) -> Result<Json<DevicesBody>, ApiError> {
    let opened = tokio::task::spawn_blocking(move || state.capture.probe_devices())
        .await
        .map_err(|error| ApiError(talkcer_core::CoreError::AudioDevice(format!("probe task failed: {error}"))))??;
    tracing::info!(devices = opened.len(), "audio devices probed");
    Ok(Json(DevicesBody { devices: opened }))
}

async fn devices(State(state): State<AppState>, Path(id): Path<Uuid>) -> Result<Json<DevicesBody>, ApiError> {
    // Opening the endpoints is blocking; keep it off the async worker.
    let opened = tokio::task::spawn_blocking(move || state.capture.probe_devices())
        .await
        .map_err(|error| ApiError(talkcer_core::CoreError::AudioDevice(format!("probe task failed: {error}"))))??;
    tracing::info!(session_id = %id, devices = opened.len(), "capture devices probed");
    Ok(Json(DevicesBody { devices: opened }))
}

/// Drive the audio pipeline for every live session until the process exits.
///
/// Every tick drains buffered PCM into transcripts and publishes them, and
/// turns a reader failure into a `capture.disconnected` event. The interval is
/// short enough to feel live and long enough to batch a sentence per tick.
pub fn spawn_pump(state: AppState) -> tokio::task::JoinHandle<()> {
    tokio::spawn(async move {
        let mut ticker = tokio::time::interval(std::time::Duration::from_millis(500));
        ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        loop {
            ticker.tick().await;
            for session_id in state.capture.live_sessions() {
                match state.capture.poll(session_id).await {
                    Ok(tick) => {
                        for saved in tick.transcripts {
                            publish_transcript(&state, &saved.item);
                        }
                        if let Some(error) = tick.disconnected {
                            state.publish(&json!({
                                "type": "capture.disconnected",
                                "session_id": session_id,
                                "error": error,
                            }));
                        }
                    }
                    Err(error) => {
                        tracing::warn!(session_id = %session_id, %error, "capture pipeline tick failed");
                    }
                }
            }
        }
    })
}

#[derive(serde::Serialize)]
struct DevicesBody {
    devices: Vec<CaptureDevice>,
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
