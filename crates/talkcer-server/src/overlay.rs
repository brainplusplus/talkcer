//! Overlay preference routes.
//!
//! These routes store window state. The desktop shell applies that state to
//! the native window.

use axum::extract::State;
use axum::{Json, Router};
use serde::Deserialize;
use talkcer_core::{OverlayPosition, OverlayPreferences, OverlayView};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

/// Overlay routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/overlay", axum::routing::get(get).put(follow))
        .route("/api/overlay/show", axum::routing::post(show))
        .route("/api/overlay/hide", axum::routing::post(hide))
        .route("/api/overlay/toggle", axum::routing::post(toggle))
        .route("/api/overlay/move", axum::routing::post(move_overlay))
        .route("/api/overlay/opacity", axum::routing::post(opacity))
        .route("/api/overlay/click-through", axum::routing::post(click_through))
        .route("/api/overlay/undetectable", axum::routing::post(undetectable))
        .route("/api/overlay/view", axum::routing::get(view))
}

#[derive(Deserialize)]
struct SessionBody {
    session_id: Uuid,
}

#[derive(Deserialize)]
struct OpacityBody {
    opacity: f64,
}

#[derive(Deserialize)]
struct ClickBody {
    enabled: bool,
}

async fn get(State(state): State<AppState>) -> Result<Json<OverlayPreferences>, ApiError> {
    Ok(Json(state.overlay.get().await?))
}

async fn show(State(state): State<AppState>) -> Result<Json<OverlayPreferences>, ApiError> {
    let preferences = state.overlay.show().await?;
    tracing::info!(state = ?preferences.state, "overlay shown");
    Ok(Json(preferences))
}

async fn hide(State(state): State<AppState>) -> Result<Json<OverlayPreferences>, ApiError> {
    let preferences = state.overlay.hide().await?;
    tracing::info!("overlay hidden");
    Ok(Json(preferences))
}

async fn toggle(State(state): State<AppState>) -> Result<Json<OverlayPreferences>, ApiError> {
    Ok(Json(state.overlay.toggle().await?))
}

async fn move_overlay(
    State(state): State<AppState>,
    Json(position): Json<OverlayPosition>,
) -> Result<Json<OverlayPreferences>, ApiError> {
    Ok(Json(state.overlay.move_to(position).await?))
}

async fn opacity(
    State(state): State<AppState>,
    Json(body): Json<OpacityBody>,
) -> Result<Json<OverlayPreferences>, ApiError> {
    Ok(Json(state.overlay.set_opacity(body.opacity).await?))
}

async fn click_through(
    State(state): State<AppState>,
    Json(body): Json<ClickBody>,
) -> Result<Json<OverlayPreferences>, ApiError> {
    Ok(Json(state.overlay.set_click_through(body.enabled).await?))
}

async fn undetectable(
    State(state): State<AppState>,
    Json(body): Json<ClickBody>,
) -> Result<Json<OverlayPreferences>, ApiError> {
    Ok(Json(state.overlay.set_undetectable(body.enabled).await?))
}

async fn follow(
    State(state): State<AppState>,
    Json(body): Json<SessionBody>,
) -> Result<Json<OverlayPreferences>, ApiError> {
    let preferences = state.overlay.follow(body.session_id).await?;
    tracing::info!(session_id = %body.session_id, "overlay session changed");
    Ok(Json(preferences))
}

async fn view(State(state): State<AppState>) -> Result<Json<OverlayView>, ApiError> {
    Ok(Json(state.overlay.view().await?))
}
