//! Session WebSocket.
//!
//! Clients subscribe, send chat, and request the local assistant stub.
//! Transcript events are produced by the HTTP intake and broadcast here.

use axum::extract::ws::{Message, WebSocket};
use axum::extract::{Query, State, WebSocketUpgrade};
use axum::response::{IntoResponse, Response};
use futures_util::{SinkExt, StreamExt};
use serde::Deserialize;
use serde_json::{json, Value};
use talkcer_core::{AssistantAction, AssistantActionRequest, CoreError, TimelineItem};
use tokio::sync::broadcast;
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

#[derive(Debug, Deserialize)]
pub struct WsQuery {
    session_id: Option<Uuid>,
}

/// Upgrade `GET /ws?session_id=<uuid>`.
pub async fn connect(
    State(state): State<AppState>,
    Query(query): Query<WsQuery>,
    ws: WebSocketUpgrade,
) -> Response {
    if let Some(session_id) = query.session_id {
        if let Err(error) = state.sessions.get(session_id).await {
            return ApiError(error).into_response();
        }
    }
    let events = state.events.subscribe();
    tracing::info!(session_id = ?query.session_id, "websocket connected");
    ws.on_upgrade(move |socket| run(socket, state, query.session_id, events))
        .into_response()
}

/// Broadcast a stored transcript without adding fields beyond the protocol.
pub fn publish_transcript(state: &AppState, item: &TimelineItem) {
    state.publish(&json!({
        "type": "transcript.updated",
        "session_id": item.session_id,
        "utterance_id": item.id,
        "speaker": item.speaker,
        "source": item.source,
        "started_at_ms": item.started_at_ms,
        "ended_at_ms": item.ended_at_ms,
        "text": item.text,
        "is_final": item.is_final,
    }));
}

/// Broadcast one stub delta and then completion. `utterance_id` stays in metadata, not the event.
pub fn publish_assistant(state: &AppState, session_id: Uuid, message: &TimelineItem) {
    state.publish(&json!({
        "type": "assistant.delta",
        "session_id": session_id,
        "message_id": message.id,
        "delta": message.text,
    }));
    state.publish(&json!({
        "type": "assistant.completed",
        "session_id": session_id,
        "message_id": message.id,
    }));
}

async fn run(
    socket: WebSocket,
    state: AppState,
    mut session_id: Option<Uuid>,
    mut events: broadcast::Receiver<String>,
) {
    let (mut write, mut read) = socket.split();
    loop {
        tokio::select! {
            incoming = read.next() => {
                match incoming {
                    Some(Ok(Message::Text(text))) => {
                        if let Some(error_event) = handle_text(&state, &mut session_id, text.as_str()).await {
                            if write.send(Message::Text(error_event.into())).await.is_err() {
                                break;
                            }
                        }
                    }
                    Some(Ok(Message::Ping(payload))) => {
                        if write.send(Message::Pong(payload)).await.is_err() {
                            break;
                        }
                    }
                    Some(Ok(Message::Close(_))) | None => break,
                    Some(Ok(_)) => {}
                    Some(Err(_)) => break,
                }
            }
            event = events.recv() => {
                match event {
                    Ok(payload) => {
                        if session_matches(session_id, &payload)
                            && write.send(Message::Text(payload.into())).await.is_err()
                        {
                            break;
                        }
                    }
                    Err(broadcast::error::RecvError::Lagged(_)) => {
                        tracing::warn!(session_id = ?session_id, "websocket listener lagged");
                    }
                    Err(broadcast::error::RecvError::Closed) => break,
                }
            }
        }
    }
    tracing::info!(session_id = ?session_id, "websocket disconnected");
}

fn session_matches(session_id: Option<Uuid>, payload: &str) -> bool {
    let Some(session_id) = session_id else {
        return false;
    };
    serde_json::from_str::<Value>(payload)
        .ok()
        .and_then(|value| value.get("session_id").and_then(Value::as_str).map(ToOwned::to_owned))
        .is_some_and(|value| value == session_id.to_string())
}

async fn handle_text(state: &AppState, session_id: &mut Option<Uuid>, text: &str) -> Option<String> {
    let value: Value = match serde_json::from_str(text) {
        Ok(value) => value,
        Err(error) => {
            return Some(error_event(*session_id, "bad_request", &error.to_string()));
        }
    };
    let event_type = value.get("type").and_then(Value::as_str).unwrap_or("");
    let result = match event_type {
        "session.subscribe" => subscribe(state, session_id, &value).await,
        "message.send" => send_message(state, session_id, &value).await,
        "assistant.action" => assistant(state, session_id, &value).await,
        _ => Err(CoreError::InvalidItemKind(event_type.to_owned())),
    };
    result.err().map(|error| {
        tracing::warn!(session_id = ?session_id, error = %error, "websocket command failed");
        error_event(*session_id, error_code(&error), &error.to_string())
    })
}

async fn subscribe(state: &AppState, session_id: &mut Option<Uuid>, value: &Value) -> Result<(), CoreError> {
    let id = required_session(value)?;
    state.sessions.get(id).await?;
    *session_id = Some(id);
    Ok(())
}

async fn send_message(state: &AppState, session_id: &mut Option<Uuid>, value: &Value) -> Result<(), CoreError> {
    let id = required_session(value)?;
    *session_id = Some(id);
    let text = value
        .get("text")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_owned();
    state.timeline.append_message(id, text, json!({})).await?;
    Ok(())
}

async fn assistant(state: &AppState, session_id: &mut Option<Uuid>, value: &Value) -> Result<(), CoreError> {
    let id = required_session(value)?;
    *session_id = Some(id);
    let action = AssistantAction::parse(value.get("action").and_then(Value::as_str).unwrap_or(""))?;
    let target = value.get("target").cloned().unwrap_or(Value::Null);
    if target.get("kind").and_then(Value::as_str) != Some("utterance") {
        return Err(CoreError::InvalidTarget);
    }
    let target_id = target
        .get("id")
        .and_then(Value::as_str)
        .ok_or(CoreError::InvalidTarget)?;
    let target_id = Uuid::parse_str(target_id).map_err(|_| CoreError::InvalidTarget)?;
    let language = value
        .get("language")
        .and_then(Value::as_str)
        .map(ToOwned::to_owned);
    let result = state
        .timeline
        .assistant_action(
            id,
            AssistantActionRequest {
                action,
                target_id,
                language,
            },
        )
        .await?;
    publish_assistant(state, id, &result.message);
    Ok(())
}

fn required_session(value: &Value) -> Result<Uuid, CoreError> {
    let raw = value
        .get("session_id")
        .and_then(Value::as_str)
        .ok_or(CoreError::NotFound)?;
    Uuid::parse_str(raw).map_err(|_| CoreError::NotFound)
}

fn error_code(error: &CoreError) -> &'static str {
    match error {
        CoreError::NotFound | CoreError::ItemNotFound => "not_found",
        CoreError::UtteranceClosed | CoreError::UtteranceSessionMismatch => "conflict",
        _ => "bad_request",
    }
}

fn error_event(session_id: Option<Uuid>, code: &str, message: &str) -> String {
    json!({
        "type": "error",
        "session_id": session_id,
        "code": code,
        "message": message,
    })
    .to_string()
}
