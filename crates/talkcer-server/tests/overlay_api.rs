//! HTTP tests for overlay state, export, consent, and the remote limit.

use axum::body::Body;
use axum::http::{header, Request, StatusCode};
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

async fn call(app: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(
        Request::builder().method(method).uri(uri).header("content-type", "application/json")
            .body(Body::from(body.map(|value| value.to_string()).unwrap_or_default())).unwrap(),
    ).await.unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() { Value::Null } else { serde_json::from_slice(&bytes).unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into_owned())) };
    (status, value)
}

#[tokio::test]
async fn overlay_export_and_remote_limit() {
    let file = path("overlay-api");
    let app = router(AppState::connect_with_limit(&file, 1).await.unwrap(), None);
    let (status, created) = call(&app, "POST", "/api/sessions", Some(json!({ "title": "Live", "mode": "live_interview" }))).await;
    assert_eq!(status, StatusCode::CREATED);
    let id = created["id"].as_str().unwrap();

    let (status, shown) = call(&app, "POST", "/api/overlay/show", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(shown["state"], "visible");
    assert_eq!(shown["always_on_top"], true);
    let (status, _) = call(&app, "POST", "/api/overlay/move", Some(json!({ "x": 30, "y": 40 }))).await;
    assert_eq!(status, StatusCode::OK);
    let (status, faded) = call(&app, "POST", "/api/overlay/opacity", Some(json!({ "opacity": 0.5 }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(faded["opacity"], 0.5);
    let (_, clicked) = call(&app, "POST", "/api/overlay/click-through", Some(json!({ "enabled": true }))).await;
    assert_eq!(clicked["state"], "click_through");
    let (status, followed) = call(&app, "PUT", "/api/overlay", Some(json!({ "session_id": id }))).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(followed["active_session_id"], id);
    let (_, hidden) = call(&app, "POST", "/api/overlay/hide", None).await;
    assert_eq!(hidden["state"], "hidden");
    assert_eq!(hidden["always_on_top"], false);

    let (status, exported) = call(&app, "GET", &format!("/api/sessions/{id}/export?format=markdown"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(exported.as_str().unwrap_or_default().contains("# Live") || exported.to_string().contains("# Live"));

    let (_, paired) = call(&app, "POST", "/api/remote/pair", Some(json!({ "enabled": true }))).await;
    let token = paired["token"].as_str().unwrap();
    let first = remote(&app, token).await;
    let second = remote(&app, token).await;
    assert_eq!(first, StatusCode::OK);
    assert_eq!(second, StatusCode::TOO_MANY_REQUESTS);
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn export_returns_download_headers_and_json_is_valid() {
    let file = path("export-headers");
    let app = router(AppState::connect(&file).await.unwrap(), None);
    let (_, created) = call(&app, "POST", "/api/sessions", Some(json!({ "title": "Notes", "mode": "learning" }))).await;
    let id = created["id"].as_str().unwrap();
    let (_, item) = call(
        &app,
        "POST",
        &format!("/api/sessions/{id}/messages"),
        Some(json!({ "kind": "user_message", "text": "Backoff question", "metadata": {} })),
    ).await;
    assert!(item["text"].as_str().unwrap_or_default().contains("Backoff"));

    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/sessions/{id}/export?format=json"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).and_then(|value| value.to_str().ok()),
        Some("application/json")
    );
    let disposition = response
        .headers()
        .get(header::CONTENT_DISPOSITION)
        .and_then(|value| value.to_str().ok())
        .unwrap_or_default()
        .to_owned();
    assert!(disposition.contains("attachment"), "{disposition}");
    assert!(disposition.contains(&format!("talkcer-session-{id}.json")), "{disposition}");
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let document: Value = serde_json::from_slice(&bytes).unwrap();
    assert_eq!(document["session"]["id"], id);
    assert!(document["items"].as_array().unwrap().iter().any(|entry| entry["text"] == "Backoff question"));

    let bad = app
        .clone()
        .oneshot(
            Request::builder()
                .method("GET")
                .uri(format!("/api/sessions/{id}/export?format=pdf"))
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(bad.status(), StatusCode::BAD_REQUEST);
    let _ = std::fs::remove_file(file);
}

async fn remote(app: &axum::Router, token: &str) -> StatusCode {
    app.clone().oneshot(
        Request::builder().method("GET").uri("/api/sessions").header("x-talkcer-client", "remote").header("authorization", format!("Bearer {token}")).body(Body::empty()).unwrap(),
    ).await.unwrap().status()
}
