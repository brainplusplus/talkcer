//! Session export routes.

use axum::extract::{Path, Query, State};
use axum::http::{header, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;
use serde::Deserialize;
use talkcer_core::{export_session, ExportFormat};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

/// Export routes.
pub fn routes() -> Router<AppState> {
    Router::new().route("/api/sessions/{id}/export", axum::routing::get(export))
}

#[derive(Deserialize)]
struct ExportQuery {
    format: String,
}

async fn export(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<ExportQuery>,
) -> Result<Response, ApiError> {
    let format = ExportFormat::parse(&query.format)?;
    let document = export_session(&state.sessions, &state.timeline, id, format).await?;
    tracing::info!(session_id = %id, format = query.format, "session exported");
    let extension = match format {
        ExportFormat::Json => "json",
        ExportFormat::Markdown => "md",
    };
    let filename = format!("talkcer-session-{id}.{extension}");
    let disposition = format!("attachment; filename=\"{filename}\"");
    Ok((
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, document.content_type),
            (header::CONTENT_DISPOSITION, disposition.as_str()),
        ],
        document.body,
    )
        .into_response())
}
