//! HTTP acceptance tests for Task 1 health and Task 2 session CRUD.

use std::path::PathBuf;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use talkcer_server::{router, AppState};
use tower::ServiceExt;
use uuid::Uuid;

fn db_path() -> PathBuf {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("api-{}.sqlite", Uuid::now_v7()))
}

async fn app() -> axum::Router {
    let path = db_path();
    let state = AppState::connect(&path).await.unwrap();
    router(state, None)
}

async fn json_response(response: axum::http::Response<Body>) -> (StatusCode, Value) {
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let value = if bytes.is_empty() {
        Value::Null
    } else {
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

async fn send(app: &axum::Router, request: Request<Body>) -> (StatusCode, Value) {
    let response = app.clone().oneshot(request).await.unwrap();
    json_response(response).await
}

#[tokio::test]
async fn health_returns_200() {
    let (status, body) = send(
        &app().await,
        Request::get("/api/health").body(Body::empty()).unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["status"], "ok");
    assert_eq!(body["app"], "talkcer");
    assert_eq!(body["version"], "0.1.0");
}

#[tokio::test]
async fn invalid_mode_is_rejected() {
    let response = app()
        .await
        .oneshot(
            Request::post("/api/sessions")
                .header("content-type", "application/json")
                .body(Body::from(
                    json!({
                        "title": "Nope",
                        "mode": "not_a_mode"
                    })
                    .to_string(),
                ))
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    let message = std::str::from_utf8(&bytes).unwrap_or("");
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
    assert!(
        message.contains("not_a_mode"),
        "rejection should name the invalid mode, got {message}"
    );
}

#[tokio::test]
async fn session_crud_and_grouping() {
    let app = app().await;
    let (status, created) = send(
        &app,
        Request::post("/api/sessions")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({
                    "title": "Backend Engineer â€” Acme",
                    "mode": "live_interview",
                    "context": {"role": "Senior Backend Engineer", "company": "Acme", "language": "en"},
                    "configuration": {"stt": "default", "thinker": "default", "tts": "disabled"}
                })
                .to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(created["status"], "ready");
    let id = created["id"].as_str().unwrap();

    let (status, loaded) = send(
        &app,
        Request::get(format!("/api/sessions/{id}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(loaded["title"], "Backend Engineer â€” Acme");

    send(
        &app,
        Request::post("/api/sessions")
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"title": "Arrays", "mode": "learning"}).to_string(),
            ))
            .unwrap(),
    )
    .await;

    let (status, listed) = send(
        &app,
        Request::get("/api/sessions?mode=live_interview&status=ready")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(listed["sessions"].as_array().unwrap().len(), 1);

    let (status, grouped) = send(
        &app,
        Request::get("/api/sessions?grouped=true")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(grouped["live_interview"].as_array().unwrap().len(), 1);
    assert_eq!(grouped["learning"].as_array().unwrap().len(), 1);
    assert_eq!(grouped["mock_interview"].as_array().unwrap().len(), 0);

    let (status, updated) = send(
        &app,
        Request::patch(format!("/api/sessions/{id}"))
            .header("content-type", "application/json")
            .body(Body::from(
                json!({"title": "Renamed", "status": "active"}).to_string(),
            ))
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["title"], "Renamed");
    assert_eq!(updated["status"], "active");

    let (status, _) = send(
        &app,
        Request::builder()
            .method("DELETE")
            .uri(format!("/api/sessions/{id}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NO_CONTENT);

    let (status, _) = send(
        &app,
        Request::get(format!("/api/sessions/{id}"))
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn invalid_filter_mode_is_rejected() {
    let (status, body) = send(
        &app().await,
        Request::get("/api/sessions?mode=nope")
            .body(Body::empty())
            .unwrap(),
    )
    .await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert!(body["error"]
        .as_str()
        .unwrap()
        .contains("invalid session mode"));
}
