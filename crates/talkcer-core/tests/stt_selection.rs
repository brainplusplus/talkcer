//! STT selection and the HTTP adapter.
//!
//! These tests pin the rules that decide which speech-to-text provider a session
//! uses, and they drive [`HttpSttTranscriber`] against a local mock server so the
//! multipart request and JSON response are verified without a real endpoint.

use std::sync::Arc;

use talkcer_core::{
    build_transcriber, credential_from_env, pcm16_to_wav, select_stt, AiConfiguration, Capability,
    CreateSession, EmbeddedTranscriber, FixedSttResolver, HttpSttTranscriber, ProviderConfig,
    ProviderKind, SaveAiConfiguration, Segment, Session, SessionMode, SttResolver, SttSelection,
    Transcriber,
};
use uuid::Uuid;

fn segment() -> Segment {
    Segment {
        started_at_ms: 0,
        ended_at_ms: 10,
        bytes: vec![0u8; 64],
        rms: 500,
        sample_rate: 48_000,
        channels: 2,
    }
}

fn config(kind: ProviderKind, provider: &str, endpoint: Option<&str>, consent: bool) -> ProviderConfig {
    ProviderConfig {
        kind,
        provider: provider.to_owned(),
        endpoint: endpoint.map(str::to_owned),
        model: "whisper-1".to_owned(),
        credentials_ref: None,
        consent,
    }
}

fn session_with_stt(stt: serde_json::Value) -> Session {
    Session {
        id: Uuid::now_v7(),
        title: "Live".to_owned(),
        mode: SessionMode::LiveInterview,
        status: talkcer_core::SessionStatus::Active,
        context: serde_json::json!({}),
        configuration: serde_json::json!({ "stt": stt }),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[test]
fn session_without_configuration_uses_the_embedded_provider() {
    let session = session_with_stt(serde_json::Value::Null);
    let selection = select_stt(&session, &[]).unwrap();
    assert_eq!(selection.provider, "embedded-stt");
    assert_eq!(selection.kind, ProviderKind::Embedded);
    assert!(!selection.is_remote());
}

#[test]
fn session_can_select_a_saved_configuration_by_id() {
    let mut stt = config(ProviderKind::SelfHosted, "self-hosted-stt", Some("http://127.0.0.1:9000/v1"), true);
    stt.model = "large-v3".to_owned();
    let saved = AiConfiguration {
        id: Uuid::now_v7(),
        name: "local whisper".to_owned(),
        stt,
        thinker: config(ProviderKind::Embedded, "embedded-thinker", None, false),
        tts: config(ProviderKind::Embedded, "disabled-tts", None, false),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let session = session_with_stt(serde_json::json!(saved.id.to_string()));
    let selection = select_stt(&session, std::slice::from_ref(&saved)).unwrap();
    assert_eq!(selection.provider, "self-hosted-stt");
    assert_eq!(selection.model, "large-v3");
    assert_eq!(selection.endpoint.as_deref(), Some("http://127.0.0.1:9000/v1"));
    assert!(selection.is_remote());
}

#[test]
fn inline_remote_selection_requires_consent_and_an_endpoint() {
    let session = session_with_stt(serde_json::json!({
        "kind": "self_hosted",
        "provider": "self-hosted-stt",
        "model": "large-v3",
        "endpoint": "http://127.0.0.1:9000/v1"
    }));
    let error = select_stt(&session, &[]).unwrap_err().to_string();
    assert!(error.contains("consent"), "{error}");

    let no_endpoint = session_with_stt(serde_json::json!({
        "kind": "self_hosted",
        "provider": "self-hosted-stt",
        "model": "large-v3",
        "consent": true
    }));
    let error = select_stt(&no_endpoint, &[]).unwrap_err().to_string();
    assert!(error.contains("endpoint"), "{error}");

    let with_consent = session_with_stt(serde_json::json!({
        "kind": "self_hosted",
        "provider": "self-hosted-stt",
        "model": "large-v3",
        "endpoint": "http://127.0.0.1:9000/v1",
        "consent": true
    }));
    let selection = select_stt(&with_consent, &[]).unwrap();
    assert_eq!(selection.kind, ProviderKind::SelfHosted);
}

#[test]
fn cloud_selection_requires_a_credential_reference() {
    let missing = session_with_stt(serde_json::json!({
        "kind": "cloud",
        "provider": "cloud-stt",
        "model": "whisper-1",
        "endpoint": "https://api.example.test/v1",
        "consent": true
    }));
    let error = select_stt(&missing, &[]).unwrap_err().to_string();
    assert!(error.contains("credentials_ref"), "{error}");

    let unknown = session_with_stt(serde_json::json!({
        "kind": "embedded",
        "provider": "not-a-provider",
        "model": "x"
    }));
    assert!(select_stt(&unknown, &[]).is_err());
}

#[test]
fn build_transcriber_matches_the_selection() {
    let embedded = build_transcriber(&SttSelection::embedded(), None).unwrap();
    assert_eq!(embedded.provider(), "embedded-stt");

    let remote = SttSelection {
        provider: "cloud-stt".to_owned(),
        kind: ProviderKind::Cloud,
        model: "whisper-1".to_owned(),
        endpoint: Some("https://api.example.test/v1".to_owned()),
        credentials_ref: Some("openai-prod".to_owned()),
        language: "en".to_owned(),
    };
    // A cloud selection without a resolved credential must not build.
    assert!(build_transcriber(&remote, None).is_err());
    let built = build_transcriber(&remote, Some("sk-not-logged".to_owned())).unwrap();
    assert_eq!(built.provider(), "http-stt");
}

#[test]
fn credential_lookup_sanitizes_the_reference() {
    std::env::set_var("TALKCER_SECRET_OPENAI_PROD", "value-1");
    assert_eq!(credential_from_env("openai-prod").as_deref(), Some("value-1"));
    std::env::remove_var("TALKCER_SECRET_OPENAI_PROD");
    assert!(credential_from_env("openai-prod").is_none());
    assert!(credential_from_env("").is_none());
}

#[test]
fn wav_header_describes_the_payload() {
    let pcm = vec![1_u8; 100];
    let wav = pcm16_to_wav(&pcm, 48_000, 2);
    assert_eq!(&wav[0..4], b"RIFF");
    assert_eq!(&wav[8..12], b"WAVE");
    assert_eq!(&wav[12..16], b"fmt ");
    assert_eq!(u32::from_le_bytes([wav[24], wav[25], wav[26], wav[27]]), 48_000);
    assert_eq!(u16::from_le_bytes([wav[22], wav[23]]), 2);
    assert_eq!(u16::from_le_bytes([wav[34], wav[35]]), 16);
    assert_eq!(&wav[36..40], b"data");
    assert_eq!(u32::from_le_bytes([wav[40], wav[41], wav[42], wav[43]]), 100);
    assert_eq!(wav.len(), 44 + 100);
}

/// A minimal HTTP server that answers one transcription request.
///
/// It records the request so the test can assert the adapter sent a multipart
/// body, then replies with the JSON shape the adapter expects.
async fn mock_stt_server() -> (String, tokio::task::JoinHandle<String>) {
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
            // Stop once the body length is satisfied.
            let text = String::from_utf8_lossy(&request);
            if let Some(headers_end) = text.find("\r\n\r\n") {
                let head = &text[..headers_end];
                let length = head
                    .lines()
                    .find_map(|line| line.to_ascii_lowercase().strip_prefix("content-length:").map(str::trim).map(str::to_owned))
                    .and_then(|value| value.parse::<usize>().ok())
                    .unwrap_or(0);
                if request.len() >= headers_end + 4 + length {
                    break;
                }
            }
        }
        let body = r#"{"text":"  hello from the endpoint  "}"#;
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
async fn http_adapter_posts_multipart_and_reads_the_text() {
    let (endpoint, server) = mock_stt_server().await;
    let transcriber = HttpSttTranscriber::new(&endpoint, "whisper-1", "en", None);
    assert_eq!(transcriber.request_url(), format!("{endpoint}/audio/transcriptions"));

    let text = transcriber.transcribe(&segment()).await.unwrap();
    assert_eq!(text.as_deref(), Some("hello from the endpoint"));

    let request = server.await.unwrap();
    assert!(request.starts_with("POST /v1/audio/transcriptions "), "{request}");
    assert!(request.to_ascii_lowercase().contains("multipart/form-data"), "{request}");
    assert!(request.contains("audio.wav"), "{request}");
    assert!(request.contains("whisper-1"), "{request}");
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

    let transcriber = HttpSttTranscriber::new(format!("http://{address}/v1"), "whisper-1", "en", None);
    let error = transcriber.transcribe(&segment()).await.unwrap_err().to_string();
    assert!(error.contains("401"), "{error}");
}

#[tokio::test]
async fn fixed_resolver_supplies_the_pipeline_transcriber() {
    let resolver = FixedSttResolver::new(Arc::new(EmbeddedTranscriber));
    let session = session_with_stt(serde_json::Value::Null);
    let transcriber = resolver.resolve(&session).await.unwrap();
    assert_eq!(transcriber.provider(), "embedded-stt");

    // The resolver trait is object safe, which the service relies on.
    let boxed: Arc<dyn SttResolver> = Arc::new(FixedSttResolver::new(Arc::new(EmbeddedTranscriber)));
    assert_eq!(boxed.resolve(&session).await.unwrap().provider(), "embedded-stt");
}

#[tokio::test]
async fn saved_configuration_round_trips_through_the_service() {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("stt-{}.sqlite", Uuid::now_v7()));
    let pool = talkcer_core::connect_and_migrate(&file).await.unwrap();
    let service = talkcer_core::AiConfigurationService::new(
        talkcer_core::SqliteAiConfigurationRepository::new(pool),
    );

    let saved = service
        .save(SaveAiConfiguration {
            id: None,
            name: "local".to_owned(),
            stt: config(ProviderKind::SelfHosted, "self-hosted-stt", Some("http://127.0.0.1:9000/v1"), true),
            thinker: config(ProviderKind::Embedded, "embedded-thinker", None, false),
            tts: config(ProviderKind::Embedded, "disabled-tts", None, false),
        })
        .await
        .unwrap();
    let result = service.test(saved.id, Capability::Stt).await.unwrap();
    assert!(result.ok, "{}", result.message);

    let listed = service.list().await.unwrap();
    let selection = select_stt(&session_with_stt(serde_json::json!(saved.id.to_string())), &listed).unwrap();
    assert_eq!(selection.provider, "self-hosted-stt");

    // A cloud slot without consent is rejected before it is stored.
    let rejected = service
        .save(SaveAiConfiguration {
            id: None,
            name: "cloud".to_owned(),
            stt: config(ProviderKind::Cloud, "cloud-stt", Some("https://api.example.test/v1"), false),
            thinker: config(ProviderKind::Embedded, "embedded-thinker", None, false),
            tts: config(ProviderKind::Embedded, "disabled-tts", None, false),
        })
        .await;
    assert!(rejected.is_err());
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn session_creation_accepts_an_stt_selection() {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("stt-session-{}.sqlite", Uuid::now_v7()));
    let pool = talkcer_core::connect_and_migrate(&file).await.unwrap();
    let sessions = talkcer_core::SessionService::new(talkcer_core::SqliteSessionRepository::new(pool));
    let session = sessions
        .create(CreateSession {
            title: "Live".to_owned(),
            mode: SessionMode::LiveInterview,
            context: serde_json::json!({}),
            configuration: serde_json::json!({
                "stt": { "kind": "embedded", "provider": "embedded-stt", "model": "embedded-utf8" }
            }),
        })
        .await
        .unwrap();
    let selection = select_stt(&session, &[]).unwrap();
    assert_eq!(selection.kind, ProviderKind::Embedded);
    let _ = std::fs::remove_file(file);
}
