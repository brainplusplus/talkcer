//! `GET /api/health`.

use axum::Json;
use serde::Serialize;

use crate::VERSION;

#[derive(Serialize)]
pub struct HealthResponse {
    status: &'static str,
    app: &'static str,
    version: &'static str,
}

/// Report that the process is serving the API.
pub async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        app: "talkcer",
        version: VERSION,
    })
}
