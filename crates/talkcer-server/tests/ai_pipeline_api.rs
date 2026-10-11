//! End-to-end AI pipeline over the HTTP API.
//!
//! A saved self-hosted thinker and a self-hosted TTS configuration point at
//! local mock servers, so these tests verify the whole seam: assistant actions,
//! mock interviews, learning turns, and voice playback all route through the
//! session's configured providers instead of the embedded stubs.

use std::sync::{Arc, Mutex};

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use talkcer_server::{router, AppState};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tower::ServiceExt;
use uuid::Uuid;

fn path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("{name}-{}.sqlite", Uuid::now_v7()))
}

async fn call(app: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
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

/// Read one full HTTP request (headers plus body) from a socket.
async fn read_request(socket: &mut tokio::net::TcpStream) -> String {
    let mut buffer = vec![0_u8; 8192];
    let mut request = Vec::new();
    loop {
        let read = socket.read(&mut buffer).await.unwrap();
        if read == 0 {
            break;
        }
        request.extend_from_slice(&buffer[..read]);
        let text = String::from_utf8_lossy(&request);
        if let Some(headers_end) = text.find("\r\n\r\n") {
            let head = &text[..headers_end];
            let length = head
                .lines()
                .find_map(|line| {
                    line.to_ascii_lowercase()
                        .strip_prefix("content-length:")
                        .map(str::trim)
                        .map(str::to_owned)
                })
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(0);
            if request.len() >= headers_end + 4 + length {
                break;
            }
        }
    }
    String::from_utf8_lossy(&request).into_owned()
}

/// A server that answers chat completions and speech requests until aborted.
///
/// Every request path is recorded in shared state so the test can assert each
/// route was exercised.
async fn mock_provider_server() -> (String, Arc<Mutex<Vec<String>>>) {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let seen = Arc::new(Mutex::new(Vec::new()));
    let recorder = Arc::clone(&seen);
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            let request = read_request(&mut socket).await;
            let is_speech = request.starts_with("POST /v1/audio/speech ");
            if let Some(line) = request.lines().next() {
                recorder.lock().unwrap().push(line.to_owned());
            }
            if is_speech {
                let body = b"RIFFremote-audio";
                let head = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: audio/wav\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
                    body.len()
                );
                let _ = socket.write_all(head.as_bytes()).await;
                let _ = socket.write_all(body).await;
            } else {
                let body = r#"{"choices":[{"message":{"role":"assistant","content":"from the remote model"}}]}"#;
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
            }
            let _ = socket.flush().await;
        }
    });
    (format!("http://{address}/v1"), seen)
}

fn remote_configuration(endpoint: &str) -> Value {
    json!({
        "name": "Remote",
        "stt": { "kind": "embedded", "provider": "embedded-stt", "model": "local" },
        "thinker": {
            "kind": "self_hosted",
            "provider": "self-hosted-thinker",
            "endpoint": endpoint,
            "model": "qwen2.5-7b",
            "consent": true
        },
        "tts": {
            "kind": "self_hosted",
            "provider": "self-hosted-tts",
            "endpoint": endpoint,
            "model": "piper",
            "consent": true
        }
    })
}

#[tokio::test]
async fn configured_thinker_and_tts_drive_every_ai_route() {
    let file = path("ai-pipeline");
    let (endpoint, seen) = mock_provider_server().await;
    let app = router(AppState::connect(&file).await.unwrap());

    let (status, saved) = call(&app, "PUT", "/api/config/ai", Some(remote_configuration(&endpoint))).await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let configuration_id = saved["id"].as_str().unwrap();

    // A live-interview session points its thinker, STT, and TTS at the saved row.
    let (status, session) = call(
        &app,
        "POST",
        "/api/sessions",
        Some(json!({
            "title": "Live",
            "mode": "live_interview",
            "configuration": {
                "thinker": configuration_id,
                "tts_provider": configuration_id,
                "tts": "enabled"
            }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    let live = session["id"].as_str().unwrap();

    // 1. A transcript, then an assistant action answered by the remote model.
    let (status, transcript) = call(
        &app,
        "POST",
        &format!("/api/sessions/{live}/transcripts"),
        Some(json!({
            "speaker": "remote",
            "source": "system_audio",
            "text": "How would you handle retries?",
            "started_at_ms": 1000,
            "ended_at_ms": 2000,
            "is_final": true
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{transcript}");
    let utterance = transcript["item"]["id"].as_str().unwrap();

    let (status, action) = call(
        &app,
        "POST",
        &format!("/api/sessions/{live}/assistant-actions"),
        Some(json!({
            "action": "explain",
            "target": { "kind": "utterance", "id": utterance }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{action}");
    assert_eq!(action["message"]["text"], "from the remote model");
    assert_eq!(action["message"]["metadata"]["provider"], "http-thinker");
    assert_eq!(action["message"]["metadata"]["stub"], false);

    // 2. TTS over the remote endpoint.
    let (status, spoken) = call(
        &app,
        "POST",
        &format!("/api/sessions/{live}/voice/speak"),
        Some(json!({ "text": "Feedback" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{spoken}");
    assert_eq!(spoken["enabled"], true);
    let audio = base64::Engine::decode(
        &base64::engine::general_purpose::STANDARD,
        spoken["audio_base64"].as_str().unwrap(),
    )
    .unwrap();
    assert!(audio.starts_with(b"RIFF"), "the embedded tone leaked through");

    // 3. A mock interview uses the same remote model.
    let (status, mock_session) = call(
        &app,
        "POST",
        "/api/sessions",
        Some(json!({
            "title": "Mock",
            "mode": "mock_interview",
            "context": { "role": "backend engineer" },
            "configuration": { "thinker": configuration_id }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{mock_session}");
    let mock_id = mock_session["id"].as_str().unwrap();
    let (status, started) = call(
        &app,
        "POST",
        &format!("/api/sessions/{mock_id}/mock"),
        Some(json!({ "action": "start" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{started}");
    assert_eq!(started["items"][0]["text"], "from the remote model");

    // 4. A learning session uses it too.
    let (status, learning_session) = call(
        &app,
        "POST",
        "/api/sessions",
        Some(json!({
            "title": "Learning",
            "mode": "learning",
            "context": { "topic": "database indexes" },
            "configuration": { "thinker": configuration_id }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{learning_session}");
    let learning_id = learning_session["id"].as_str().unwrap();
    let (status, learning) = call(
        &app,
        "POST",
        &format!("/api/sessions/{learning_id}/learning"),
        Some(json!({ "action": "start" })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{learning}");
    assert_eq!(learning["items"][0]["text"], "from the remote model");

    // Every route reached the remote endpoint, and speech was distinct from chat.
    let lines = seen.lock().unwrap().clone();
    assert!(
        lines.iter().filter(|line| line.starts_with("POST /v1/chat/completions ")).count() >= 4,
        "expected four chat completions, saw {lines:?}"
    );
    assert!(
        lines.iter().any(|line| line.starts_with("POST /v1/audio/speech ")),
        "expected a speech request, saw {lines:?}"
    );
    let _ = std::fs::remove_file(file);
}
