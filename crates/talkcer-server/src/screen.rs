//! Screen capture routes.
//!
//! `POST /api/sessions/{id}/screen` captures the whole current screen and, when
//! `direct` is set, reads it with the vision model and answers in the session's
//! mode. Without `direct` the capture is staged and nothing is sent.
//!
//! `POST /api/sessions/{id}/screen/send` answers from every staged capture plus
//! an optional note, then clears the staged captures.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::Json;
use serde::Deserialize;
use talkcer_core::{
    is_pending, mark_sent, pending_screens, record_screen, HttpVision, ScreenCapturer,
    ScreenIntent, TimelineItem, TimelineQuery, VisionReader,
};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use crate::ws::publish_assistant;

/// Screen routes mounted under `/api`.
pub fn routes() -> axum::Router<AppState> {
    axum::Router::new()
        .route("/api/sessions/{id}/screen", axum::routing::post(capture))
        .route("/api/sessions/{id}/screen/send", axum::routing::post(send))
}

#[derive(Debug, Deserialize)]
struct CaptureBody {
    /// Read and answer now. Absent means stage the capture for a later send.
    #[serde(default)]
    direct: bool,
}

#[derive(Debug, Deserialize, Default)]
struct SendBody {
    /// Extra context the user typed before sending.
    #[serde(default)]
    note: Option<String>,
}

async fn capture(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<CaptureBody>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let session = state.sessions.get(id).await?;
    let frame = windows_capturer()?.capture().await?;
    let intent = if body.direct { ScreenIntent::Direct } else { ScreenIntent::Stage };
    let (reading, reply) = match intent {
        ScreenIntent::Direct => {
            let reading = vision()?.read(&frame).await?;
            let task = talkcer_core::screen_task(session.mode, &reading, None);
            let answer = state.timeline.think(&session, &task).await?;
            (Some(reading), Some(answer.text))
        }
        ScreenIntent::Stage => (None, None),
    };
    let outcome = record_screen(&state.timeline, id, intent, &frame, reading.as_deref(), reply.as_deref()).await?;
    if let Some(reply) = &outcome.reply {
        publish_assistant(&state, id, reply);
    }
    Ok(Json(serde_json::json!({
        "captured": outcome.captured,
        "reply": outcome.reply,
    })))
}

async fn send(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<SendBody>,
) -> Result<Json<TimelineItem>, ApiError> {
    let session = state.sessions.get(id).await?;
    let page = state
        .timeline
        .list(id, TimelineQuery { limit: 200, before: None })
        .await?;
    let staged = pending_screens(&page.items);
    if staged.is_empty() && body.note.as_deref().unwrap_or("").trim().is_empty() {
        return Err(ApiError::from(talkcer_core::CoreError::Screen(
            "nothing staged to send".to_owned(),
        )));
    }
    let reading = staged
        .iter()
        .map(|item| item.text.as_str())
        .collect::<Vec<_>>()
        .join("\n");
    let task = talkcer_core::screen_task(session.mode, &reading, body.note.as_deref());
    let answer = state.timeline.think(&session, &task).await?;
    for item in &staged {
        let mut metadata = item.metadata.clone();
        mark_sent(&mut metadata);
        state.timeline.set_metadata(item.id, metadata).await?;
    }
    let outcome = record_screen(
        &state.timeline,
        id,
        ScreenIntent::Direct,
        &talkcer_core::ScreenFrame { png: Vec::new(), width: 0, height: 0 },
        Some(&reading),
        Some(&answer.text),
    )
    .await?;
    let reply = outcome.reply.expect("a send stores a reply");
    publish_assistant(&state, id, &reply);
    let _ = is_pending(&reply);
    Ok(Json(reply))
}

/// The vision model, from `TALKCER_VISION_ENDPOINT` and `TALKCER_VISION_MODEL`.
fn vision() -> Result<HttpVision, ApiError> {
    let endpoint = std::env::var("TALKCER_VISION_ENDPOINT").map_err(|_| {
        ApiError::from(talkcer_core::CoreError::Screen(
            "set TALKCER_VISION_ENDPOINT to read screens".to_owned(),
        ))
    })?;
    let model = std::env::var("TALKCER_VISION_MODEL").unwrap_or_else(|_| "vision".to_owned());
    let credential = std::env::var("TALKCER_VISION_KEY").ok();
    Ok(HttpVision::new(endpoint, model, credential))
}

/// The full-screen capturer for this platform.
fn windows_capturer() -> Result<Arc<dyn ScreenCapturer>, ApiError> {
    #[cfg(windows)]
    {
        Ok(Arc::new(crate::screen_capture::WindowsScreen::new()))
    }
    #[cfg(not(windows))]
    {
        Err(ApiError::from(talkcer_core::CoreError::Screen(
            "screen capture is only available on Windows".to_owned(),
        )))
    }
}
