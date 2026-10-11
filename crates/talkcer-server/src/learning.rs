//! Text learning HTTP route.
//!
//! The controller does not call a model. AI rows use the existing assistant events.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::{Json, Router};
use talkcer_core::{ItemKind, LearningTurn, LearningTurnResult};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use crate::ws::publish_assistant;

/// Learning routes.
pub fn routes() -> Router<AppState> {
    Router::new().route("/api/sessions/{id}/learning", axum::routing::post(turn))
}

async fn turn(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<LearningTurn>,
) -> Result<(StatusCode, Json<LearningTurnResult>), ApiError> {
    let result = state.learning.turn(id, input).await?;
    for item in &result.items {
        if matches!(item.kind, ItemKind::AiMessage | ItemKind::Feedback) {
            publish_assistant(&state, id, item);
        }
    }
    tracing::info!(session_id = %id, status = result.status.as_str(), items = result.items.len(), "learning turn stored");
    Ok((StatusCode::OK, Json(result)))
}
