//! Remote pairing routes and the gate for a second computer.
//!
//! Loopback requests remain local. A non-loopback peer, or a request marked
//! `x-talkcer-client: remote`, must present the pairing token.

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, Request, State};
use axum::middleware::Next;
use axum::response::Response;
use axum::{Json, Router};
use talkcer_core::{CoreError, PairRequest, Pairing, RemoteStatus};

use crate::error::ApiError;
use crate::state::AppState;

/// Pairing routes. These stay available to the local computer.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/remote/status", axum::routing::get(status))
        .route("/api/remote/pair", axum::routing::post(pair))
}

async fn status(State(state): State<AppState>) -> Result<Json<RemoteStatus>, ApiError> {
    Ok(Json(state.remote.status().await?))
}

async fn pair(
    State(state): State<AppState>,
    Json(input): Json<PairRequest>,
) -> Result<Json<Pairing>, ApiError> {
    let pairing = state.remote.pair(input.enabled).await?;
    tracing::info!(enabled = pairing.enabled, "remote pairing changed");
    Ok(Json(pairing))
}

/// Reject a second computer until pairing is enabled and the token matches.
///
/// # Errors
///
/// Returns [`ApiError`] with `401` when the remote client is not authorized.
pub async fn authorize(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    if request.uri().path().starts_with("/api/remote/") {
        return Ok(next.run(request).await);
    }
    let marked_remote = request
        .headers()
        .get("x-talkcer-client")
        .and_then(|value| value.to_str().ok())
        == Some("remote");
    let outside_loopback = request
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .is_some_and(|info| !info.0.ip().is_loopback());
    if !marked_remote && !outside_loopback {
        return Ok(next.run(request).await);
    }
    let header = request
        .headers()
        .get(axum::http::header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or("");
    let Some(token) = header.strip_prefix("Bearer ").filter(|token| !token.is_empty()) else {
        return Err(ApiError(CoreError::Unauthorized));
    };
    state.remote.authorize(token).await?;
    state.rate.lock().await.allow(token)?;
    Ok(next.run(request).await)
}
