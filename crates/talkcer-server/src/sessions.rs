//! Session HTTP routes.
//!
//! `GET /api/sessions?grouped=true` returns sessions grouped by mode.
//! A query `mode` or `status` that is not in the contract returns 400.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use talkcer_core::{
    CoreError, CreateSession, Session, SessionFilter, SessionMode, SessionStatus, SessionsByMode,
    UpdateSession,
};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

/// Session routes mounted under `/api`.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/sessions", get(list).post(create))
        .route(
            "/api/sessions/{id}",
            get(get_one).patch(update).delete(delete),
        )
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    mode: Option<String>,
    status: Option<String>,
    grouped: Option<bool>,
}

async fn create(
    State(state): State<AppState>,
    Json(input): Json<CreateSession>,
) -> Result<(StatusCode, Json<Session>), ApiError> {
    let session = state.sessions.create(input).await?;
    Ok((StatusCode::CREATED, Json(session)))
}

async fn list(
    State(state): State<AppState>,
    Query(query): Query<ListQuery>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let filter = SessionFilter {
        mode: match query.mode.as_deref() {
            None => None,
            Some(value) => Some(SessionMode::parse(value).map_err(ApiError::from)?),
        },
        status: match query.status.as_deref() {
            None => None,
            Some(value) => Some(SessionStatus::parse(value).map_err(ApiError::from)?),
        },
    };

    if query.grouped.unwrap_or(false) {
        if filter.mode.is_some() || filter.status.is_some() {
            return Err(ApiError(CoreError::GroupedFilterConflict));
        }
        let grouped: SessionsByMode = state.sessions.grouped_by_mode().await?;
        return Ok(Json(serde_json::to_value(grouped).expect("sessions serialize")));
    }

    let sessions = state.sessions.list(&filter).await?;
    Ok(Json(serde_json::json!({ "sessions": sessions })))
}

async fn get_one(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<Json<Session>, ApiError> {
    Ok(Json(state.sessions.get(id).await?))
}

async fn update(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpdateSession>,
) -> Result<Json<Session>, ApiError> {
    Ok(Json(state.sessions.update(id, input).await?))
}

async fn delete(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> Result<StatusCode, ApiError> {
    state.sessions.delete(id).await?;
    Ok(StatusCode::NO_CONTENT)
}
