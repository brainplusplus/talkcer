//! HTTP tests for AI configuration and the text mock interview.

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

async fn http(
    app: &axum::Router,
    method: &str,
    uri: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .method(method)
                .uri(uri)
                .header("content-type", "application/json")
                .body(Body::from(
                    body.map(|value| value.to_string()).unwrap_or_default(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes)
            .unwrap_or(Value::String(String::from_utf8_lossy(&bytes).into_owned()))
    };
    (status, value)
}

fn configuration() -> Value {
    json!({
        "name": "Default",
        "stt": { "kind": "embedded", "provider": "embedded-stt", "model": "local" },
        "thinker": {
            "kind": "cloud",
            "provider": "cloud-thinker",
            "endpoint": "https://llm.example",
            "model": "talkcer-thinker",
            "credentials_ref": "THINKER_KEY",
            "consent": true
        },
        "tts": {
            "kind": "self_hosted",
            "provider": "self-hosted-tts",
            "endpoint": "http://127.0.0.1:5002",
            "model": "talkcer-voice"
        }
    })
}

#[tokio::test]
async fn configuration_round_trip_and_provider_catalog() {
    let file = path("config");
    let app = router(AppState::connect(&file).await.unwrap(), None);
    let (status, providers) = http(&app, "GET", "/api/config/ai/providers", None).await;
    assert_eq!(status, StatusCode::OK);
    assert!(providers
        .as_array()
        .unwrap()
        .iter()
        .any(|item| item["provider"] == "embedded-stt"
            && item["capabilities"]["supports_partial_transcript"] == true));

    let (status, saved) = http(&app, "PUT", "/api/config/ai", Some(configuration())).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let id = saved["id"].as_str().unwrap();

    let (status, rejected) = http(
        &app,
        "PUT",
        "/api/config/ai",
        Some(json!({
            "name": "Bad",
            "stt": { "kind": "cloud", "provider": "embedded-stt", "model": "local" },
            "thinker": configuration()["thinker"],
            "tts": configuration()["tts"]
        })),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST, "{rejected}");
    assert!(rejected["error"].as_str().unwrap().contains("embedded"));

    let (status, tested) = http(
        &app,
        "POST",
        &format!("/api/config/ai/{id}/test"),
        Some(json!({ "capability": "thinker" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{tested}");
    assert_eq!(tested["ok"], true);

    drop(app);
    let reopened = router(AppState::connect(&file).await.unwrap(), None);
    let (status, listed) = http(&reopened, "GET", "/api/config/ai", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed[0]["id"], id);
    assert_eq!(listed[0]["thinker"]["credentials_ref"], "THINKER_KEY");
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn mock_interview_transcript_survives_restart() {
    let file = path("mock");
    let app = router(AppState::connect(&file).await.unwrap(), None);
    let (status, session) = http(
        &app,
        "POST",
        "/api/sessions",
        Some(json!({ "title": "Mock", "mode": "mock_interview", "context": { "role": "backend engineer" } })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    let id = session["id"].as_str().unwrap();

    let (status, started) = http(
        &app,
        "POST",
        &format!("/api/sessions/{id}/mock"),
        Some(json!({ "action": "start" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");
    assert_eq!(started["status"], "active");

    let (status, answered) = http(
        &app,
        "POST",
        &format!("/api/sessions/{id}/mock"),
        Some(json!({ "action": "answer", "text": "I separated retries from the request path." })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{answered}");
    assert_eq!(answered["items"][1]["metadata"]["turn"], "follow_up");

    let (status, completed) = http(
        &app,
        "POST",
        &format!("/api/sessions/{id}/mock"),
        Some(json!({ "action": "complete", "text": "I would keep retries bounded." })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{completed}");
    assert_eq!(completed["status"], "completed");

    drop(app);
    let reopened = router(AppState::connect(&file).await.unwrap(), None);
    let (status, page) = http(
        &reopened,
        "GET",
        &format!("/api/sessions/{id}/timeline"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let turns: Vec<_> = page["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|item| item["metadata"]["turn"].as_str().unwrap())
        .collect();
    assert_eq!(
        turns,
        [
            "question",
            "answer",
            "follow_up",
            "answer",
            "feedback",
            "summary"
        ]
    );
    let (status, loaded) = http(&reopened, "GET", &format!("/api/sessions/{id}"), None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(loaded["status"], "completed");
    let _ = std::fs::remove_file(file);
}
