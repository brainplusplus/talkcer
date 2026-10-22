//! HTTP and WebSocket acceptance tests for Task 3.

use std::time::Duration;

use axum::body::Body;
use axum::http::{Request, StatusCode};
use futures_util::{Stream, StreamExt};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use talkcer_server::{router, AppState};
use tokio::net::TcpListener;
use tokio_tungstenite::connect_async;
use tokio_tungstenite::tungstenite::Message;
use tower::ServiceExt;
use uuid::Uuid;

fn db_path(name: &str) -> std::path::PathBuf {
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
        serde_json::from_slice(&bytes).unwrap()
    };
    (status, value)
}

async fn next_json<S>(stream: &mut S) -> Value
where
    S: Stream<Item = Result<Message, tokio_tungstenite::tungstenite::Error>> + Unpin,
{
    loop {
        let message = tokio::time::timeout(Duration::from_secs(2), stream.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        if let Message::Text(text) = message {
            return serde_json::from_str(text.as_str()).unwrap();
        }
    }
}

#[tokio::test]
async fn missing_timeline_is_not_found() {
    let path = db_path("missing");
    let app = router(AppState::connect(&path).await.unwrap(), None);
    let (status, _) = http(
        &app,
        "GET",
        &format!("/api/sessions/{}/timeline", Uuid::now_v7()),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn partials_final_websocket_and_assistant_stub() {
    let path = db_path("stream");
    let app = router(AppState::connect(&path).await.unwrap(), None);
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let served = app.clone();
    let server = tokio::spawn(async move {
        axum::serve(listener, served).await.unwrap();
    });

    let (status, session) = http(
        &app,
        "POST",
        "/api/sessions",
        Some(json!({"title": "Live", "mode": "live_interview"})),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED);
    let session_id = session["id"].as_str().unwrap();

    let (mut socket, _) = connect_async(format!("ws://{address}/ws?session_id={session_id}"))
        .await
        .unwrap();

    let (status, created) = http(
        &app,
        "POST",
        &format!("/api/sessions/{session_id}/transcripts"),
        Some(json!({
            "speaker": "remote",
            "source": "system_audio",
            "text": "How",
            "started_at_ms": 12000,
            "is_final": false
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{created}");
    let utterance_id = created["utterance_id"].as_str().unwrap().to_owned();

    for text in ["How would", "How would you"] {
        let (status, updated) = http(
            &app,
            "POST",
            &format!("/api/sessions/{session_id}/transcripts"),
            Some(json!({
                "utterance_id": utterance_id,
                "speaker": "local_user",
                "source": "microphone",
                "text": text,
                "started_at_ms": 99000,
                "is_final": false
            })),
        )
        .await;
        assert_eq!(status, StatusCode::OK, "{updated}");
        assert_eq!(updated["created"], false);
        assert_eq!(updated["item"]["speaker"], "remote");
        assert_eq!(updated["item"]["started_at_ms"], 12000);
    }

    for expected in ["How", "How would", "How would you"] {
        let partial = next_json(&mut socket).await;
        assert_eq!(partial["type"], "transcript.updated");
        assert_eq!(partial["session_id"], session_id);
        assert_eq!(partial["utterance_id"], utterance_id);
        assert_eq!(partial["text"], expected);
        assert_eq!(partial["is_final"], false);
    }

    let (status, finished) = http(
        &app,
        "POST",
        &format!("/api/sessions/{session_id}/transcripts"),
        Some(json!({
            "utterance_id": utterance_id,
            "speaker": "remote",
            "source": "system_audio",
            "text": "How would you handle retries?",
            "started_at_ms": 12000,
            "ended_at_ms": 15800,
            "is_final": true
        })),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{finished}");
    let event = next_json(&mut socket).await;
    assert_eq!(event["text"], "How would you handle retries?");
    assert_eq!(event["is_final"], true);
    assert_eq!(event["utterance_id"], utterance_id);

    let (status, page) = http(
        &app,
        "GET",
        &format!("/api/sessions/{session_id}/timeline"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(page["items"].as_array().unwrap().len(), 1);

    let (status, conflict) = http(
        &app,
        "POST",
        &format!("/api/sessions/{session_id}/transcripts"),
        Some(json!({
            "utterance_id": utterance_id,
            "speaker": "remote",
            "source": "system_audio",
            "text": "late",
            "started_at_ms": 12000,
            "is_final": false
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CONFLICT, "{conflict}");

    let (status, assistant) = http(
        &app,
        "POST",
        &format!("/api/sessions/{session_id}/assistant-actions"),
        Some(json!({
            "action": "explain",
            "target": { "kind": "utterance", "id": utterance_id },
            "language": "id"
        })),
    )
    .await;
    assert_eq!(status, StatusCode::CREATED, "{assistant}");
    let delta = next_json(&mut socket).await;
    let completed = next_json(&mut socket).await;
    assert_eq!(delta["type"], "assistant.delta");
    assert_eq!(delta["session_id"], session_id);
    assert!(delta.get("utterance_id").is_none());
    assert_eq!(completed["type"], "assistant.completed");
    assert_eq!(completed["message_id"], delta["message_id"]);
    assert!(completed.get("utterance_id").is_none());

    let (status, page) = http(
        &app,
        "GET",
        &format!("/api/sessions/{session_id}/timeline"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    let items = page["items"].as_array().unwrap();
    assert_eq!(items.len(), 3);
    assert_eq!(items[0]["kind"], "transcript");
    assert_eq!(items[1]["purpose"], "assistant_instruction");
    assert_eq!(items[2]["kind"], "ai_message");
    assert_eq!(items[2]["metadata"]["target"]["id"], utterance_id);
    assert!(items[2]["metadata"].get("utterance_id").is_none());

    let reread = router(AppState::connect(&path).await.unwrap(), None);
    let (status, persisted) = http(
        &reread,
        "GET",
        &format!("/api/sessions/{session_id}/timeline"),
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(
        persisted["items"][0]["text"],
        "How would you handle retries?"
    );

    server.abort();
    let _ = std::fs::remove_file(path);
}
