//! HTTP tests for Windows capture fixtures and remote pairing.

use axum::body::Body;
use axum::http::{Request, StatusCode};
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

async fn call(app: &axum::Router, method: &str, uri: &str, body: Option<Value>, remote_token: Option<&str>) -> (StatusCode, Value) {
    let mut builder = Request::builder().method(method).uri(uri).header("content-type", "application/json");
    if let Some(token) = remote_token {
        builder = builder.header("x-talkcer-client", "remote").header("authorization", format!("Bearer {token}"));
    }
    let response = app
        .clone()
        .oneshot(builder.body(Body::from(body.map(|value| value.to_string()).unwrap_or_default())).unwrap())
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into_owned())) };
    (status, value)
}

#[tokio::test]
async fn capture_channels_and_remote_gate() {
    let file = path("live-remote");
    let app = router(AppState::connect(&file).await.unwrap());
    let (status, denied) = call(&app, "GET", "/api/sessions", None, Some("missing")).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{denied}");

    let (status, pairing) = call(&app, "POST", "/api/remote/pair", Some(json!({ "enabled": true })), None).await;
    assert_eq!(status, StatusCode::OK, "{pairing}");
    let token = pairing["token"].as_str().unwrap();

    let (status, session) = call(&app, "POST", "/api/sessions", Some(json!({ "title": "Remote live", "mode": "live_interview" })), Some(token)).await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    let id = session["id"].as_str().unwrap();
    let (status, listed) = call(&app, "GET", "/api/sessions?mode=live_interview", None, Some(token)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["sessions"][0]["id"], id);

    let (status, started) = call(&app, "POST", &format!("/api/sessions/{id}/capture"), Some(json!({ "action": "start" })), None).await;
    assert_eq!(status, StatusCode::OK, "{started}");
    assert_eq!(started["platform"], "windows");
    let (status, local) = call(&app, "POST", &format!("/api/sessions/{id}/capture/fixture"), Some(json!({ "channel": "microphone", "text": "I can hear you.", "started_at_ms": 10, "is_final": true })), None).await;
    let (status_remote, remote_audio) = call(&app, "POST", &format!("/api/sessions/{id}/capture/fixture"), Some(json!({ "channel": "system_audio", "text": "Tell me about retries.", "started_at_ms": 20, "is_final": true })), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(status_remote, StatusCode::OK);
    assert_ne!(local["item"]["id"], remote_audio["item"]["id"]);

    let (_, paused) = call(&app, "POST", &format!("/api/sessions/{id}/capture"), Some(json!({ "action": "pause" })), None).await;
    assert_eq!(paused["state"], "paused");
    let (_, resumed) = call(&app, "POST", &format!("/api/sessions/{id}/capture"), Some(json!({ "action": "resume" })), None).await;
    assert_eq!(resumed["state"], "capturing");
    let (_, broken) = call(&app, "POST", &format!("/api/sessions/{id}/capture"), Some(json!({ "action": "disconnect" })), None).await;
    assert_eq!(broken["state"], "error");
    let (status, recovered) = call(&app, "POST", &format!("/api/sessions/{id}/capture"), Some(json!({ "action": "recover" })), None).await;
    assert_eq!(recovered["state"], "idle");
    assert_eq!(status, StatusCode::OK);

    let utterance = remote_audio["item"]["id"].as_str().unwrap();
    let (status, action) = call(
        &app,
        "POST",
        &format!("/api/sessions/{id}/assistant-actions"),
        Some(json!({ "action": "explain", "target": { "kind": "utterance", "id": utterance } })),
        Some(token),
    ).await;
    assert_eq!(status, StatusCode::CREATED, "{action}");
    let (status, page) = call(&app, "GET", &format!("/api/sessions/{id}/timeline"), None, Some(token)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(page["items"].as_array().unwrap().len() >= 3);

    let (_, disabled) = call(&app, "POST", "/api/remote/pair", Some(json!({ "enabled": false })), None).await;
    assert_eq!(disabled["enabled"], false);
    let (status, rejected) = call(&app, "GET", "/api/sessions", None, Some(token)).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{rejected}");
    let _ = std::fs::remove_file(file);
}
