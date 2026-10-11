//! Browser voice HTTP routes.
//!
//! Audio is transcribed by the mock adapter. TTS returns a local WAV only when
//! the session configuration enables it.

use axum::extract::{Path, State};
use axum::{Json, Router};
use talkcer_core::{SpeakRequest, SpeechAudio, TranscriptUpsert, VoiceAudio};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use crate::ws::publish_transcript;

/// Voice routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/sessions/{id}/voice/audio", axum::routing::post(audio))
        .route("/api/sessions/{id}/voice/speak", axum::routing::post(speak))
}

async fn audio(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<VoiceAudio>,
) -> Result<Json<TranscriptUpsert>, ApiError> {
    let saved = state.voice.transcribe(id, input).await?;
    publish_transcript(&state, &saved.item);
    tracing::info!(
        session_id = %id,
        utterance_id = %saved.item.id,
        is_final = saved.item.is_final,
        "voice transcript stored"
    );
    Ok(Json(saved))
}

async fn speak(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<SpeakRequest>,
) -> Result<Json<SpeechAudio>, ApiError> {
    let audio = state.voice.speak(id, input).await?;
    tracing::info!(session_id = %id, enabled = audio.enabled, "voice playback prepared");
    Ok(Json(audio))
}
