//! Timeline HTTP routes.
//!
//! Transcripts enter through HTTP and are broadcast to WebSocket subscribers.
//! Assistant actions write a local stub and never call a provider.

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::get;
use axum::{Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use talkcer_core::{
    AssistantAction, AssistantActionRequest, CoreError, ItemKind, TimelineItem, TimelineQuery,
    UpsertTranscript,
};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use crate::ws::{publish_assistant, publish_transcript};

/// Timeline routes mounted under `/api`.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/sessions/{id}/transcripts", axum::routing::post(upsert_transcript))
        .route("/api/sessions/{id}/timeline", get(list_timeline))
        .route("/api/sessions/{id}/messages", axum::routing::post(create_message))
        .route(
            "/api/sessions/{id}/assistant-actions",
            axum::routing::post(assistant_action),
        )
}

#[derive(Debug, Deserialize)]
struct TimelineQueryParams {
    limit: Option<u32>,
    before: Option<Uuid>,
}

#[derive(Debug, Deserialize)]
struct MessageBody {
    kind: String,
    text: String,
    #[serde(default)]
    metadata: Value,
}

#[derive(Debug, Deserialize)]
struct AssistantBody {
    action: AssistantAction,
    target: AssistantTarget,
    language: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AssistantTarget {
    kind: String,
    id: Uuid,
}

async fn upsert_transcript(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(input): Json<UpsertTranscript>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    let saved = state.timeline.upsert_transcript(id, input).await?;
    tracing::info!(
        session_id = %id,
        utterance_id = %saved.item.id,
        is_final = saved.item.is_final,
        created = saved.created,
        "transcript stored"
    );
    publish_transcript(&state, &saved.item);
    let status = if saved.created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((
        status,
        Json(json!({
            "utterance_id": saved.item.id,
            "created": saved.created,
            "item": saved.item,
        })),
    ))
}

async fn list_timeline(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Query(query): Query<TimelineQueryParams>,
) -> Result<Json<talkcer_core::TimelinePage>, ApiError> {
    let page = state
        .timeline
        .list(
            id,
            TimelineQuery {
                limit: query.limit.unwrap_or(100),
                before: query.before,
            },
        )
        .await?;
    Ok(Json(page))
}

async fn create_message(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<MessageBody>,
) -> Result<(StatusCode, Json<TimelineItem>), ApiError> {
    if body.kind != ItemKind::UserMessage.as_str() {
        return Err(ApiError(CoreError::InvalidItemKind(body.kind)));
    }
    let item = state
        .timeline
        .append_message(id, body.text, body.metadata)
        .await?;
    tracing::info!(session_id = %id, message_id = %item.id, "user message stored");
    Ok((StatusCode::CREATED, Json(item)))
}

async fn assistant_action(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<AssistantBody>,
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if body.target.kind != "utterance" {
        return Err(ApiError(CoreError::InvalidTarget));
    }
    let result = state
        .timeline
        .assistant_action(
            id,
            AssistantActionRequest {
                action: body.action,
                target_id: body.target.id,
                language: body.language,
            },
        )
        .await?;
    tracing::info!(
        session_id = %id,
        message_id = %result.message.id,
        action = body.action.as_str(),
        "assistant reply stored"
    );
    publish_assistant(&state, id, &result.message);
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "instruction": result.instruction,
            "message": result.message,
        })),
    ))
}
