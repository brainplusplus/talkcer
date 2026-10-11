//! AI configuration HTTP routes.
//!
//! Secrets stay out of the stored configuration. Provider tests only check
//! the saved configuration against the registry.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Deserialize;
use talkcer_core::{AiConfiguration, Capability, ProviderDescriptor, ProviderTestResult, SaveAiConfiguration};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

/// Routes mounted under `/api/config/ai`.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/config/ai", get(list).put(save))
        .route("/api/config/ai/providers", get(providers))
        .route("/api/config/ai/{id}/test", post(test))
}

async fn list(State(state): State<AppState>) -> Result<Json<Vec<AiConfiguration>>, ApiError> {
    Ok(Json(state.ai.list().await?))
}

async fn providers(State(state): State<AppState>) -> Json<Vec<ProviderDescriptor>> {
    Json(state.ai.providers().to_vec())
}

async fn save(
    State(state): State<AppState>,
    Json(input): Json<SaveAiConfiguration>,
) -> Result<(StatusCode, Json<AiConfiguration>), ApiError> {
    let created = input.id.is_none();
    let saved = state.ai.save(input).await?;
    tracing::info!(configuration_id = %saved.id, created, "ai configuration stored");
    let status = if created { StatusCode::CREATED } else { StatusCode::OK };
    Ok((status, Json(saved)))
}

#[derive(Debug, Deserialize)]
struct TestBody {
    capability: Capability,
}

async fn test(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<TestBody>,
) -> Result<Json<ProviderTestResult>, ApiError> {
    let result = state.ai.test(id, body.capability).await?;
    tracing::info!(
        configuration_id = %id,
        capability = result.capability.as_str(),
        ok = result.ok,
        "ai provider tested"
    );
    Ok(Json(result))
}
