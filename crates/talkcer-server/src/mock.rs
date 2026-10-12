//! Text mock interview HTTP route.
//!
//! The controller stores each generated row and broadcasts the existing
//! assistant events for AI output. Text comes from the session's configured
//! thinker, falling back to the embedded offline templates.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Json, Router};
use talkcer_core::{ItemKind, MockTurn, MockTurnResult};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use crate::ws::publish_assistant;

/// Mock interview routes.
pub fn routes() -> Router<AppState> {
    Router::new().route("/api/sessions/{id}/mock", axum::routing::post(turn))
}

async fn turn(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<MockTurn>,
) -> Result<(StatusCode, Json<MockTurnResult>), ApiError> {
    let result = state.mock.turn(id, input).await?;
    for item in &result.items {
        if matches!(item.kind, ItemKind::AiMessage | ItemKind::Feedback) {
            publish_assistant(&state, id, item);
        }
    }
    tracing::info!(session_id = %id, status = result.status.as_str(), items = result.items.len(), "mock turn stored");
    Ok((StatusCode::OK, Json(result)))
}
