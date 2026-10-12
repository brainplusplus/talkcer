//! Thinker selection and the HTTP adapter.
//!
//! These tests pin the rules that decide which language model a session uses,
//! and they drive [`HttpThinker`] against a local mock server so the chat
//! request and JSON response are verified without a real endpoint.

use std::sync::Arc;

use talkcer_core::{
    build_thinker, select_thinker, AiConfiguration, CreateSession, EmbeddedThinker,
    FixedThinkerResolver, HttpThinker, ProviderConfig, ProviderKind, SaveAiConfiguration, Session,
    SessionMode, Thinker, ThinkerResolver, ThinkerSelection, ThinkerTask,
};
use uuid::Uuid;

fn config(kind: ProviderKind, provider: &str, endpoint: Option<&str>, consent: bool) -> ProviderConfig {
    ProviderConfig {
        kind,
        provider: provider.to_owned(),
        endpoint: endpoint.map(str::to_owned),
        model: "gpt-4o-mini".to_owned(),
        credentials_ref: None,
        consent,
    }
}

fn session_with_thinker(thinker: serde_json::Value) -> Session {
    Session {
        id: Uuid::now_v7(),
        title: "Live".to_owned(),
        mode: SessionMode::LiveInterview,
        status: talkcer_core::SessionStatus::Active,
        context: serde_json::json!({}),
        configuration: serde_json::json!({ "thinker": thinker }),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

fn assistant_task() -> ThinkerTask {
    ThinkerTask::AssistantAction {
        action: talkcer_core::AssistantAction::Explain,
        target: "explain retries".to_owned(),
        language: Some("id".to_owned()),
    }
}

#[test]
fn session_without_configuration_uses_the_embedded_provider() {
    let session = session_with_thinker(serde_json::Value::Null);
    let selection = select_thinker(&session, &[]).unwrap();
    assert_eq!(selection.provider, "embedded-thinker");
    assert_eq!(selection.kind, ProviderKind::Embedded);
    assert!(!selection.is_remote());
}

#[test]
fn session_can_select_a_saved_configuration_by_id() {
    let mut thinker = config(
        ProviderKind::SelfHosted,
        "self-hosted-thinker",
        Some("http://127.0.0.1:9000/v1"),
        true,
    );
    thinker.model = "qwen2.5-7b".to_owned();
    let saved = AiConfiguration {
        id: Uuid::now_v7(),
        name: "local model".to_owned(),
        stt: config(ProviderKind::Embedded, "embedded-stt", None, false),
        thinker,
        tts: config(ProviderKind::Embedded, "disabled-tts", None, false),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let session = session_with_thinker(serde_json::json!(saved.id.to_string()));
    let selection = select_thinker(&session, std::slice::from_ref(&saved)).unwrap();
    assert_eq!(selection.provider, "self-hosted-thinker");
    assert_eq!(selection.model, "qwen2.5-7b");
    assert_eq!(selection.endpoint.as_deref(), Some("http://127.0.0.1:9000/v1"));
    assert!(selection.is_remote());
}

#[test]
fn inline_remote_selection_requires_consent_and_an_endpoint() {
    let session = session_with_thinker(serde_json::json!({
        "kind": "self_hosted",
        "provider": "self-hosted-thinker",
        "model": "qwen2.5-7b",
        "endpoint": "http://127.0.0.1:9000/v1"
    }));
    let error = select_thinker(&session, &[]).unwrap_err().to_string();
    assert!(error.contains("consent"), "{error}");

    let no_endpoint = session_with_thinker(serde_json::json!({
        "kind": "self_hosted",
        "provider": "self-hosted-thinker",
        "model": "qwen2.5-7b",
        "consent": true
    }));
    let error = select_thinker(&no_endpoint, &[]).unwrap_err().to_string();
    assert!(error.contains("endpoint"), "{error}");

    let with_consent = session_with_thinker(serde_json::json!({
        "kind": "self_hosted",
        "provider": "self-hosted-thinker",
        "model": "qwen2.5-7b",
        "endpoint": "http://127.0.0.1:9000/v1",
        "consent": true
    }));
    let selection = select_thinker(&with_consent, &[]).unwrap();
    assert_eq!(selection.kind, ProviderKind::SelfHosted);
}

#[test]
fn cloud_selection_requires_a_credential_reference() {
    let missing = session_with_thinker(serde_json::json!({
        "kind": "cloud",
        "provider": "cloud-thinker",
        "model": "gpt-4o-mini",
        "endpoint": "https://api.example.test/v1",
        "consent": true
    }));
    let error = select_thinker(&missing, &[]).unwrap_err().to_string();
    assert!(error.contains("credentials_ref"), "{error}");

    let unknown = session_with_thinker(serde_json::json!({
        "kind": "embedded",
        "provider": "not-a-provider",
        "model": "x"
    }));
    assert!(select_thinker(&unknown, &[]).is_err());
}

#[test]
fn build_thinker_matches_the_selection() {
    let embedded = build_thinker(&ThinkerSelection::embedded(), None).unwrap();
    assert_eq!(embedded.provider(), "embedded-thinker");

    let remote = ThinkerSelection {
        provider: "cloud-thinker".to_owned(),
        kind: ProviderKind::Cloud,
        model: "gpt-4o-mini".to_owned(),
        endpoint: Some("https://api.example.test/v1".to_owned()),
        credentials_ref: Some("openai-prod".to_owned()),
        language: "en".to_owned(),
    };
    // A cloud selection without a resolved credential must not build.
    assert!(build_thinker(&remote, None).is_err());
    let built = build_thinker(&remote, Some("sk-not-logged".to_owned())).unwrap();
    assert_eq!(built.provider(), "http-thinker");
}

#[tokio::test]
async fn embedded_thinker_matches_the_offline_templates() {
    let thinker = EmbeddedThinker;
    let reply = thinker.complete(&assistant_task()).await.unwrap();
    assert_eq!(reply.provider, "embedded-thinker");
    assert_eq!(
        reply.text,
        talkcer_core::stub_assistant_text(
            talkcer_core::AssistantAction::Explain,
            Uuid::nil(),
        )
        .replace(&Uuid::nil().to_string(), "explain retries"),
    );

    let question = thinker
        .complete(&ThinkerTask::MockQuestion {
            role: "SRE".to_owned(),
        })
        .await
        .unwrap();
    assert!(question.text.contains("SRE"), "{}", question.text);
    let summary = thinker.complete(&ThinkerTask::MockSummary).await.unwrap();
    assert!(summary.text.starts_with("Summary"), "{}", summary.text);
}

/// A minimal HTTP server that answers one chat completion request.
///
/// It records the request so the test can assert the adapter sent a JSON body,
/// then replies with the shape the adapter expects.
async fn mock_chat_server() -> (String, tokio::task::JoinHandle<String>) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let handle = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
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
        let body = r#"{"choices":[{"message":{"role":"assistant","content":"  A short answer.  "}}]}"#;
        let response = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: application/json\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{}",
            body.len(),
            body
        );
        socket.write_all(response.as_bytes()).await.unwrap();
        socket.flush().await.unwrap();
        String::from_utf8_lossy(&request).into_owned()
    });
    (format!("http://{address}/v1"), handle)
}

#[tokio::test]
async fn http_adapter_posts_a_chat_body_and_reads_the_text() {
    let (endpoint, server) = mock_chat_server().await;
    let thinker = HttpThinker::new(&endpoint, "gpt-4o-mini", "en", None);
    assert_eq!(thinker.request_url(), format!("{endpoint}/chat/completions"));

    let reply = thinker.complete(&assistant_task()).await.unwrap();
    assert_eq!(reply.text, "A short answer.");
    assert_eq!(reply.provider, "http-thinker");

    let request = server.await.unwrap();
    assert!(request.starts_with("POST /v1/chat/completions "), "{request}");
    assert!(request.to_ascii_lowercase().contains("application/json"), "{request}");
    assert!(request.contains("gpt-4o-mini"), "{request}");
    assert!(request.contains("explain retries"), "{request}");
}

#[tokio::test]
async fn http_adapter_surfaces_a_non_success_status() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let mut buffer = vec![0_u8; 4096];
        let _ = socket.read(&mut buffer).await.unwrap();
        let body = "unauthorized";
        let response = format!(
            "HTTP/1.1 401 Unauthorized\r\ncontent-length: {}\r\nconnection: close\r\n\r\n{body}",
            body.len()
        );
        socket.write_all(response.as_bytes()).await.unwrap();
    });

    let thinker = HttpThinker::new(format!("http://{address}/v1"), "gpt-4o-mini", "en", None);
    let error = thinker.complete(&assistant_task()).await.unwrap_err().to_string();
    assert!(error.contains("401"), "{error}");
}

#[tokio::test]
async fn fixed_resolver_supplies_the_service_thinker() {
    let resolver = FixedThinkerResolver::new(Arc::new(EmbeddedThinker));
    let session = session_with_thinker(serde_json::Value::Null);
    let thinker = resolver.resolve(&session).await.unwrap();
    assert_eq!(thinker.provider(), "embedded-thinker");

    // The resolver trait is object safe, which the services rely on.
    let boxed: Arc<dyn ThinkerResolver> = Arc::new(FixedThinkerResolver::new(Arc::new(EmbeddedThinker)));
    assert_eq!(boxed.resolve(&session).await.unwrap().provider(), "embedded-thinker");
}

#[tokio::test]
async fn saved_configuration_round_trips_through_the_service() {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("thinker-{}.sqlite", Uuid::now_v7()));
    let pool = talkcer_core::connect_and_migrate(&file).await.unwrap();
    let service = talkcer_core::AiConfigurationService::new(
        talkcer_core::SqliteAiConfigurationRepository::new(pool),
    );

    let saved = service
        .save(SaveAiConfiguration {
            id: None,
            name: "local".to_owned(),
            stt: config(ProviderKind::Embedded, "embedded-stt", None, false),
            thinker: config(
                ProviderKind::SelfHosted,
                "self-hosted-thinker",
                Some("http://127.0.0.1:9000/v1"),
                true,
            ),
            tts: config(ProviderKind::Embedded, "disabled-tts", None, false),
        })
        .await
        .unwrap();
    let result = service.test(saved.id, talkcer_core::Capability::Thinker).await.unwrap();
    assert!(result.ok, "{}", result.message);

    let listed = service.list().await.unwrap();
    let selection =
        select_thinker(&session_with_thinker(serde_json::json!(saved.id.to_string())), &listed).unwrap();
    assert_eq!(selection.provider, "self-hosted-thinker");

    // A cloud slot without consent is rejected before it is stored.
    let rejected = service
        .save(SaveAiConfiguration {
            id: None,
            name: "cloud".to_owned(),
            stt: config(ProviderKind::Embedded, "embedded-stt", None, false),
            thinker: config(
                ProviderKind::Cloud,
                "cloud-thinker",
                Some("https://api.example.test/v1"),
                false,
            ),
            tts: config(ProviderKind::Embedded, "disabled-tts", None, false),
        })
        .await;
    assert!(rejected.is_err());
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn session_creation_accepts_a_thinker_selection() {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("thinker-session-{}.sqlite", Uuid::now_v7()));
    let pool = talkcer_core::connect_and_migrate(&file).await.unwrap();
    let sessions = talkcer_core::SessionService::new(talkcer_core::SqliteSessionRepository::new(pool));
    let session = sessions
        .create(CreateSession {
            title: "Live".to_owned(),
            mode: SessionMode::LiveInterview,
            context: serde_json::json!({}),
            configuration: serde_json::json!({
                "thinker": { "kind": "embedded", "provider": "embedded-thinker", "model": "embedded-template" }
            }),
        })
        .await
        .unwrap();
    let selection = select_thinker(&session, &[]).unwrap();
    assert_eq!(selection.kind, ProviderKind::Embedded);
    let _ = std::fs::remove_file(file);
}
