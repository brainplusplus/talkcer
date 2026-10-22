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
    build_thinker, credential_from_env, select_model, AssistantAction, AssistantActionRequest,
    CoreError, ItemKind, Purpose, Speaker, ThinkerTask, TimelineItem, TimelineQuery,
    UpsertTranscript,
};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;
use crate::ws::{publish_assistant, publish_transcript};

/// Timeline routes mounted under `/api`.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route(
            "/api/sessions/{id}/transcripts",
            axum::routing::post(upsert_transcript),
        )
        .route("/api/sessions/{id}/timeline", get(list_timeline))
        .route(
            "/api/sessions/{id}/messages",
            axum::routing::post(create_message),
        )
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
    /// `provider:model` override. Absent values use the session thinker.
    #[serde(default)]
    model: Option<String>,
    /// Prepend the stored profile (without birth date or place) to the prompt.
    #[serde(default)]
    profile_context: bool,
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
) -> Result<(StatusCode, Json<Value>), ApiError> {
    if body.kind != ItemKind::UserMessage.as_str() {
        return Err(ApiError(CoreError::InvalidItemKind(body.kind)));
    }
    let profile_context = body.profile_context;
    let model = body.model;
    let item = state
        .timeline
        .append_message(id, body.text.clone(), body.metadata)
        .await?;
    tracing::info!(session_id = %id, message_id = %item.id, "user message stored");
    let reply = match answer(&state, id, &body.text, model.as_deref(), profile_context).await {
        Ok(reply) => Some(reply),
        Err(error) => {
            tracing::warn!(session_id = %id, error = %error, "chat reply failed");
            None
        }
    };
    Ok((
        StatusCode::CREATED,
        Json(json!({
            "message": item,
            "reply": reply,
        })),
    ))
}

/// Generate one reply, store it, and publish it. Validation errors from the
/// user message itself are returned before this runs.
async fn answer(
    state: &AppState,
    id: Uuid,
    text: &str,
    model: Option<&str>,
    profile_context: bool,
) -> Result<TimelineItem, CoreError> {
    let session = state.sessions.get(id).await?;
    let mut prompt = text.trim().to_owned();
    if profile_context {
        let profile = state.profile.get().await?;
        let context = profile.context_text();
        if !context.is_empty() {
            prompt = format!("Profile:\n{context}\n\n{prompt}");
        }
    }
    let task = ThinkerTask::Chat { prompt };
    let reply = if let Some((provider, model)) = split_model(model) {
        let thinker = override_thinker(state, provider, model).await?;
        thinker.complete(&task).await?
    } else {
        state.timeline.think(&session, &task).await?
    };
    let model_label = model.unwrap_or(reply.provider.as_str()).to_owned();
    let item = state
        .timeline
        .append_item(
            id,
            ItemKind::AiMessage,
            Speaker::Ai,
            Purpose::AssistantResponse,
            reply.text,
            json!({ "provider": reply.provider, "model": model_label }),
        )
        .await?;
    publish_assistant(state, id, &item);
    Ok(item)
}

/// `provider:model` with both halves non-empty.
fn split_model(model: Option<&str>) -> Option<(&str, &str)> {
    let (provider, model) = model?.split_once(':')?;
    let provider = provider.trim();
    let model = model.trim();
    if provider.is_empty() || model.is_empty() {
        None
    } else {
        Some((provider, model))
    }
}

/// Build a thinker for one request's model without changing the session.
async fn override_thinker(
    state: &AppState,
    provider: &str,
    model: &str,
) -> Result<std::sync::Arc<dyn talkcer_core::Thinker>, CoreError> {
    let configurations = state.ai.list().await?;
    let selection = select_model(provider, model, &configurations)?;
    let credential = selection
        .credentials_ref
        .as_deref()
        .and_then(credential_from_env);
    build_thinker(&selection, credential)
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
