//! HTTP tests for learning continuation and browser voice.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine;
use http_body_util::BodyExt;
use serde_json::{json, Value};
use talkcer_server::{router, AppState};
use tower::ServiceExt;
use uuid::Uuid;

fn path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("{name}-{}.sqlite", Uuid::now_v7()))
}

async fn http(app: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(body.map(|value| value.to_string()).unwrap_or_default()))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    (status, value)
}

#[tokio::test]
async fn learning_history_survives_reopen() {
    let file = path("learning");
    let app = router(AppState::connect(&file).await.unwrap(), None);
    let (status, session) = http(
        &app,
        "POST",
        "/api/sessions",
        Some(json!({
            "title": "PostgreSQL",
            "mode": "learning",
            "context": { "topic": "PostgreSQL query optimization" }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    let id = session["id"].as_str().unwrap();
    let (status, started) = http(&app, "POST", &format!("/api/sessions/{id}/learning"), Some(json!({ "action": "start" }))).await;
    assert_eq!(status, StatusCode::OK, "{started}");
    http(
        &app,
        "POST",
        &format!("/api/sessions/{id}/learning"),
        Some(json!({ "action": "discuss", "text": "When should I add an index?" })),
    )
    .await;
    http(
        &app,
        "POST",
        &format!("/api/sessions/{id}/learning"),
        Some(json!({ "action": "answer", "text": "When the filter is selective." })),
    )
    .await;
    drop(app);

    let reopened = router(AppState::connect(&file).await.unwrap(), None);
    let (status, continued) = http(
        &reopened,
        "POST",
        &format!("/api/sessions/{id}/learning"),
        Some(json!({ "action": "discuss", "text": "How do I read EXPLAIN?" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{continued}");
    let (status, page) = http(&reopened, "GET", &format!("/api/sessions/{id}/timeline"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["items"].as_array().unwrap().len(), 10);
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn voice_partial_final_and_tts_playback() {
    let file = path("voice");
    let app = router(AppState::connect(&file).await.unwrap(), None);
    let (status, session) = http(
        &app,
        "POST",
        "/api/sessions",
        Some(json!({ "title": "Voice", "mode": "learning", "configuration": { "tts": "disabled" } })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    let id = session["id"].as_str().unwrap();
    let chunk = base64::engine::general_purpose::STANDARD.encode("How");
    let (status, partial) = http(
        &app,
        "POST",
        &format!("/api/sessions/{id}/voice/audio"),
        Some(json!({
            "audio_source": "browser_mic",
            "chunks": [chunk],
            "started_at_ms": 1000,
            "is_final": false
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{partial}");
    let utterance = partial["item"]["id"].as_str().unwrap();
    let (status, finished) = http(
        &app,
        "POST",
        &format!("/api/sessions/{id}/voice/audio"),
        Some(json!({
            "utterance_id": utterance,
            "audio_source": "mock_audio",
            "text": "How does an index help?",
            "started_at_ms": 1000,
            "is_final": true
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{finished}");
    assert_eq!(finished["item"]["is_final"], true);
    assert_eq!(finished["item"]["metadata"]["audio_source"], "browser_mic");

    let (status, silent) = http(
        &app,
        "POST",
        &format!("/api/sessions/{id}/voice/speak"),
        Some(json!({ "text": "Feedback" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{silent}");
    assert_eq!(silent["enabled"], false);

    http(
        &app,
        "PATCH",
        &format!("/api/sessions/{id}"),
        Some(json!({ "configuration": { "tts": "enabled" } })),
    )
    .await;
    let (status, spoken) = http(
        &app,
        "POST",
        &format!("/api/sessions/{id}/voice/speak"),
        Some(json!({ "text": "Feedback" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{spoken}");
    assert_eq!(spoken["enabled"], true);
    assert!(spoken["audio_base64"].as_str().unwrap().starts_with("Ukl"));
    let _ = std::fs::remove_file(file);
}
