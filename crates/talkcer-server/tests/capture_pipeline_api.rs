//! End-to-end pipeline test over the HTTP API.
//!
//! A scripted device provider stands in for real audio, and the background pump
//! drains it into transcripts. This covers the wiring the web client relies on:
//! start capture over HTTP, then observe a transcribed timeline.

use std::sync::Arc;
use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use talkcer_core::{utf8_pcm, CaptureDevice, ScriptedDeviceProvider};
use talkcer_server::{router, spawn_pump, AppState};
use tower::ServiceExt;
use uuid::Uuid;

fn path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("{name}-{}.sqlite", Uuid::now_v7()))
}

fn scripted() -> ScriptedDeviceProvider {
    ScriptedDeviceProvider::spoken_pair("I can hear you.", "Tell me about retries.")
}

fn device(channel: &str) -> CaptureDevice {
    CaptureDevice {
        channel: channel.to_owned(),
        name: format!("Scripted {channel}"),
        direction: if channel == "system_audio" { "render_loopback" } else { "capture" }.to_owned(),
        state: "active".to_owned(),
        sample_rate: 48_000,
        channels: 2,
        frames: 0,
    }
}

async fn call(app: &axum::Router, method: &str, uri: &str, body: Option<Value>) -> (StatusCode, Value) {
    let request = Request::builder()
        .method(method)
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.map(|value| value.to_string()).unwrap_or_default()))
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
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
async fn pump_transcribes_scripted_audio_into_the_timeline() {
    let file = path("pipeline-api");
    let state = AppState::connect_with_capture_provider(&file, 30, Arc::new(scripted())).await.unwrap();
    let app = router(state.clone());
    let pump = spawn_pump(state);

    let (status, session) = call(&app, "POST", "/api/sessions", Some(json!({ "title": "Live", "mode": "live_interview" }))).await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    let id = session["id"].as_str().unwrap();

    let (status, started) = call(&app, "POST", &format!("/api/sessions/{id}/capture"), Some(json!({ "action": "start" }))).await;
    assert_eq!(status, StatusCode::OK, "{started}");
    assert_eq!(started["devices"].as_array().unwrap().len(), 2);

    // Give the pump a few ticks to drain the scripted buffers.
    let mut items = Vec::new();
    for _ in 0..20 {
        tokio::time::sleep(Duration::from_millis(150)).await;
        let (_, page) = call(&app, "GET", &format!("/api/sessions/{id}/timeline"), None).await;
        items = page["items"].as_array().cloned().unwrap_or_default();
        if items.len() >= 2 {
            break;
        }
    }
    pump.abort();

    let speakers: Vec<&str> = items.iter().filter_map(|item| item["speaker"].as_str()).collect();
    assert!(speakers.contains(&"local_user"), "timeline: {items:?}");
    assert!(speakers.contains(&"remote"), "timeline: {items:?}");
    let texts: Vec<&str> = items.iter().filter_map(|item| item["text"].as_str()).collect();
    assert!(texts.contains(&"I can hear you."), "timeline: {items:?}");
    assert!(texts.contains(&"Tell me about retries."), "timeline: {items:?}");

    let (_, stopped) = call(&app, "POST", &format!("/api/sessions/{id}/capture"), Some(json!({ "action": "stop" }))).await;
    assert_eq!(stopped["state"], "idle");
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn global_audio_devices_route_reports_the_provider() {
    let file = path("devices-api");
    let provider = ScriptedDeviceProvider::new(vec![
        (device("microphone"), utf8_pcm("a")),
        (device("system_audio"), utf8_pcm("b")),
    ]);
    let state = AppState::connect_with_capture_provider(&file, 30, Arc::new(provider)).await.unwrap();
    let app = router(state);

    let (status, body) = call(&app, "GET", "/api/devices/audio", None).await;
    assert_eq!(status, StatusCode::OK, "{body}");
    assert_eq!(body["devices"].as_array().unwrap().len(), 2);
    assert_eq!(body["devices"][1]["direction"], "render_loopback");
    let _ = std::fs::remove_file(file);
}

/// A mock STT endpoint that answers every request with the same text.
async fn mock_stt_endpoint(text: &'static str) -> String {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            let Ok((mut socket, _)) = listener.accept().await else {
                break;
            };
            tokio::spawn(async move {
                let mut buffer = vec![0_u8; 16384];
                let mut request = Vec::new();
                loop {
                    let Ok(read) = socket.read(&mut buffer).await else {
                        return;
                    };
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
                let body = format!("{{\"text\":\"{text}\"}}");
                let response = format!(
                    "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
                    body.len(),
                    body
                );
                let _ = socket.write_all(response.as_bytes()).await;
                let _ = socket.flush().await;
            });
        }
    });
    format!("http://{address}/v1")
}

#[tokio::test]
async fn session_stt_selection_drives_the_pipeline_end_to_end() {
    let file = path("stt-api");
    let endpoint = mock_stt_endpoint("recognized by the endpoint").await;

    // Both channels are silent to the embedded adapter, so any text proves the
    // configured HTTP endpoint was used.
    let provider = ScriptedDeviceProvider::new(vec![
        (device("microphone"), vec![9_u8; 48_000 * 4]),
        (device("system_audio"), vec![9_u8; 48_000 * 4]),
    ]);
    let state = AppState::connect_with_capture_provider(&file, 30, Arc::new(provider)).await.unwrap();
    let app = router(state.clone());
    let pump = spawn_pump(state);

    // Save a self-hosted STT configuration, then point the session at it.
    let (status, saved) = call(
        &app,
        "PUT",
        "/api/config/ai",
        Some(json!({
            "name": "local whisper",
            "stt": {
                "kind": "self_hosted",
                "provider": "self-hosted-stt",
                "model": "large-v3",
                "endpoint": endpoint,
                "consent": true
            },
            "thinker": { "kind": "embedded", "provider": "embedded-thinker", "model": "stub" },
            "tts": { "kind": "embedded", "provider": "disabled-tts", "model": "none" }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{saved}");
    let configuration_id = saved["id"].as_str().unwrap();

    let (status, session) = call(
        &app,
        "POST",
        "/api/sessions",
        Some(json!({
            "title": "Live",
            "mode": "live_interview",
            "configuration": { "stt": configuration_id }
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{session}");
    let id = session["id"].as_str().unwrap();

    let (status, started) = call(&app, "POST", &format!("/api/sessions/{id}/capture"), Some(json!({ "action": "start" }))).await;
    assert_eq!(status, StatusCode::OK, "{started}");

    let mut items = Vec::new();
    for _ in 0..30 {
        tokio::time::sleep(Duration::from_millis(200)).await;
        let (_, page) = call(&app, "GET", &format!("/api/sessions/{id}/timeline"), None).await;
        items = page["items"].as_array().cloned().unwrap_or_default();
        if items.len() >= 2 {
            break;
        }
    }
    pump.abort();

    assert!(items.len() >= 2, "the configured endpoint produced no transcript: {items:?}");
    for item in &items {
        assert_eq!(item["text"], "recognized by the endpoint");
        assert_eq!(item["metadata"]["stt_provider"], "http-stt");
    }
    let speakers: Vec<&str> = items.iter().filter_map(|item| item["speaker"].as_str()).collect();
    assert!(speakers.contains(&"local_user"), "{items:?}");
    assert!(speakers.contains(&"remote"), "{items:?}");
    let _ = std::fs::remove_file(file);
}
