//! TTS selection and the HTTP adapter.
//!
//! These tests pin the rules that decide which speech-synthesis provider a
//! session uses, and they drive [`HttpTts`] against a local mock server so the
//! request and audio response are verified without a real endpoint.

use std::sync::Arc;

use base64::Engine;
use talkcer_core::{
    build_tts, select_tts, AiConfiguration, CreateSession, EmbeddedTts, FixedTtsResolver, HttpTts,
    ProviderConfig, ProviderKind, SaveAiConfiguration, Session, SessionMode, SessionService,
    SpeakRequest, SqliteSessionRepository, SqliteTimelineRepository, TimelineService, Tts,
    TtsResolver, TtsSelection, UpdateSession, VoiceService,
};
use uuid::Uuid;

fn config(
    kind: ProviderKind,
    provider: &str,
    endpoint: Option<&str>,
    consent: bool,
) -> ProviderConfig {
    ProviderConfig {
        kind,
        provider: provider.to_owned(),
        endpoint: endpoint.map(str::to_owned),
        model: "tts-1".to_owned(),
        credentials_ref: None,
        consent,
    }
}

fn session_with_tts(tts_provider: serde_json::Value) -> Session {
    Session {
        id: Uuid::now_v7(),
        title: "Live".to_owned(),
        mode: SessionMode::LiveInterview,
        status: talkcer_core::SessionStatus::Active,
        context: serde_json::json!({}),
        configuration: serde_json::json!({ "tts": "enabled", "tts_provider": tts_provider }),
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    }
}

#[test]
fn session_without_configuration_uses_the_embedded_provider() {
    let session = session_with_tts(serde_json::Value::Null);
    let selection = select_tts(&session, &[]).unwrap();
    assert_eq!(selection.provider, "embedded-tts");
    assert_eq!(selection.kind, ProviderKind::Embedded);
    assert!(!selection.is_remote());
}

#[test]
fn the_session_page_voice_is_read_from_tts_model() {
    // The session page writes `provider:model` under `tts_model`, which the
    // resolver ignored while it read `tts_provider` only. Piper is a local
    // voice, but it is served over an endpoint, so without a saved
    // configuration the selection must fail as "needs a configuration" rather
    // than as an unknown provider.
    let session = Session {
        configuration: serde_json::json!({ "tts_model": "piper:en_US-lessac-medium" }),
        ..session_with_tts(serde_json::Value::Null)
    };
    let error = select_tts(&session, &[]).unwrap_err().to_string();
    assert!(error.contains("needs a saved configuration"), "{error}");
    assert!(!error.contains("unknown"), "{error}");
}

#[test]
fn a_local_tts_model_with_a_saved_configuration_is_selected() {
    let mut saved = config(
        ProviderKind::SelfHosted,
        "piper",
        Some("http://127.0.0.1:9100/v1"),
        true,
    );
    saved.model = "en_US-lessac-medium".to_owned();
    let saved = AiConfiguration {
        id: Uuid::now_v7(),
        name: "local piper".to_owned(),
        stt: config(ProviderKind::Embedded, "embedded-stt", None, false),
        thinker: config(ProviderKind::Embedded, "embedded-thinker", None, false),
        tts: saved,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let session = Session {
        configuration: serde_json::json!({ "tts_model": "piper:en_US-lessac-medium" }),
        ..session_with_tts(serde_json::Value::Null)
    };
    let selection = select_tts(&session, std::slice::from_ref(&saved)).unwrap();
    assert_eq!(selection.provider, "piper");
    assert_eq!(selection.model, "en_US-lessac-medium");
    assert_eq!(
        selection.endpoint.as_deref(),
        Some("http://127.0.0.1:9100/v1")
    );
}

#[test]
fn a_tts_model_choice_uses_the_saved_configuration_for_its_provider() {
    let mut saved = config(
        ProviderKind::Cloud,
        "tts-openai",
        Some("https://api.openai.test/v1"),
        true,
    );
    saved.credentials_ref = Some("openai-prod".to_owned());
    let saved = AiConfiguration {
        id: Uuid::now_v7(),
        name: "openai voice".to_owned(),
        stt: config(ProviderKind::Embedded, "embedded-stt", None, false),
        thinker: config(ProviderKind::Embedded, "embedded-thinker", None, false),
        tts: saved,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let session = Session {
        configuration: serde_json::json!({ "tts_model": "tts-openai:gpt-4o-mini-tts" }),
        ..session_with_tts(serde_json::Value::Null)
    };
    let selection = select_tts(&session, std::slice::from_ref(&saved)).unwrap();
    assert_eq!(selection.provider, "tts-openai");
    assert_eq!(selection.model, "gpt-4o-mini-tts");
    assert_eq!(
        selection.endpoint.as_deref(),
        Some("https://api.openai.test/v1")
    );
    assert!(selection.is_remote());
}

#[test]
fn session_can_select_a_saved_configuration_by_id() {
    let mut tts = config(
        ProviderKind::SelfHosted,
        "self-hosted-tts",
        Some("http://127.0.0.1:9000/v1"),
        true,
    );
    tts.model = "piper".to_owned();
    let saved = AiConfiguration {
        id: Uuid::now_v7(),
        name: "local voice".to_owned(),
        stt: config(ProviderKind::Embedded, "embedded-stt", None, false),
        thinker: config(ProviderKind::Embedded, "embedded-thinker", None, false),
        tts,
        created_at: chrono::Utc::now(),
        updated_at: chrono::Utc::now(),
    };
    let session = session_with_tts(serde_json::json!(saved.id.to_string()));
    let selection = select_tts(&session, std::slice::from_ref(&saved)).unwrap();
    assert_eq!(selection.provider, "self-hosted-tts");
    assert_eq!(selection.model, "piper");
    assert_eq!(
        selection.endpoint.as_deref(),
        Some("http://127.0.0.1:9000/v1")
    );
    assert!(selection.is_remote());
}

#[test]
fn inline_remote_selection_requires_consent_and_an_endpoint() {
    let session = session_with_tts(serde_json::json!({
        "kind": "self_hosted",
        "provider": "self-hosted-tts",
        "model": "piper",
        "endpoint": "http://127.0.0.1:9000/v1"
    }));
    let error = select_tts(&session, &[]).unwrap_err().to_string();
    assert!(error.contains("consent"), "{error}");

    let no_endpoint = session_with_tts(serde_json::json!({
        "kind": "self_hosted",
        "provider": "self-hosted-tts",
        "model": "piper",
        "consent": true
    }));
    let error = select_tts(&no_endpoint, &[]).unwrap_err().to_string();
    assert!(error.contains("endpoint"), "{error}");

    let with_consent = session_with_tts(serde_json::json!({
        "kind": "self_hosted",
        "provider": "self-hosted-tts",
        "model": "piper",
        "endpoint": "http://127.0.0.1:9000/v1",
        "consent": true
    }));
    let selection = select_tts(&with_consent, &[]).unwrap();
    assert_eq!(selection.kind, ProviderKind::SelfHosted);
}

#[test]
fn cloud_selection_requires_a_credential_reference() {
    let missing = session_with_tts(serde_json::json!({
        "kind": "cloud",
        "provider": "cloud-tts",
        "model": "tts-1",
        "endpoint": "https://api.example.test/v1",
        "consent": true
    }));
    let error = select_tts(&missing, &[]).unwrap_err().to_string();
    assert!(error.contains("credentials_ref"), "{error}");

    let unknown = session_with_tts(serde_json::json!({
        "kind": "embedded",
        "provider": "not-a-provider",
        "model": "x"
    }));
    assert!(select_tts(&unknown, &[]).is_err());
}

#[test]
fn build_tts_matches_the_selection() {
    let embedded = build_tts(&TtsSelection::embedded(), None).unwrap();
    assert_eq!(embedded.provider(), "embedded-tts");

    let remote = TtsSelection {
        provider: "cloud-tts".to_owned(),
        kind: ProviderKind::Cloud,
        model: "tts-1".to_owned(),
        endpoint: Some("https://api.example.test/v1".to_owned()),
        credentials_ref: Some("openai-prod".to_owned()),
        voice: "alloy".to_owned(),
    };
    // A cloud selection without a resolved credential must not build.
    assert!(build_tts(&remote, None).is_err());
    let built = build_tts(&remote, Some("sk-not-logged".to_owned())).unwrap();
    assert_eq!(built.provider(), "http-tts");
}

#[tokio::test]
async fn embedded_tts_returns_a_wav_whose_tone_follows_the_text() {
    let tts = EmbeddedTts;
    let output = tts.synthesize("Feedback").await.unwrap();
    assert_eq!(output.content_type, "audio/wav");
    assert!(output.bytes.starts_with(b"RIFF"));
    assert_eq!(&output.bytes[8..12], b"WAVE");

    let longer = tts.synthesize("A longer piece of feedback").await.unwrap();
    assert_ne!(output.bytes, longer.bytes);
}

/// A minimal HTTP server that answers one synthesis request with audio bytes.
async fn mock_speech_server() -> (String, tokio::task::JoinHandle<String>) {
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
        let body = b"RIFFaudio-bytes";
        let head = format!(
            "HTTP/1.1 200 OK\r\ncontent-type: audio/wav\r\ncontent-length: {}\r\nconnection: close\r\n\r\n",
            body.len()
        );
        socket.write_all(head.as_bytes()).await.unwrap();
        socket.write_all(body).await.unwrap();
        socket.flush().await.unwrap();
        String::from_utf8_lossy(&request).into_owned()
    });
    (format!("http://{address}/v1"), handle)
}

#[tokio::test]
async fn http_adapter_posts_a_speech_body_and_reads_the_audio() {
    let (endpoint, server) = mock_speech_server().await;
    let tts = HttpTts::new(&endpoint, "tts-1", "alloy", None);
    assert_eq!(tts.request_url(), format!("{endpoint}/audio/speech"));

    let output = tts.synthesize("Feedback").await.unwrap();
    assert_eq!(output.content_type, "audio/wav");
    assert_eq!(output.bytes, b"RIFFaudio-bytes");

    let request = server.await.unwrap();
    assert!(request.starts_with("POST /v1/audio/speech "), "{request}");
    assert!(
        request.to_ascii_lowercase().contains("application/json"),
        "{request}"
    );
    assert!(request.contains("tts-1"), "{request}");
    assert!(request.contains("alloy"), "{request}");
    assert!(request.contains("Feedback"), "{request}");
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

    let tts = HttpTts::new(format!("http://{address}/v1"), "tts-1", "alloy", None);
    let error = tts.synthesize("Feedback").await.unwrap_err().to_string();
    assert!(error.contains("401"), "{error}");
}

#[tokio::test]
async fn fixed_resolver_supplies_the_service_adapter() {
    let resolver = FixedTtsResolver::new(Arc::new(EmbeddedTts));
    let session = session_with_tts(serde_json::Value::Null);
    let tts = resolver.resolve(&session).await.unwrap();
    assert_eq!(tts.provider(), "embedded-tts");

    // The resolver trait is object safe, which the service relies on.
    let boxed: Arc<dyn TtsResolver> = Arc::new(FixedTtsResolver::new(Arc::new(EmbeddedTts)));
    assert_eq!(
        boxed.resolve(&session).await.unwrap().provider(),
        "embedded-tts"
    );
}

#[tokio::test]
async fn voice_speak_uses_the_session_tts_adapter() {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("tts-voice-{}.sqlite", Uuid::now_v7()));
    let pool = talkcer_core::connect_and_migrate(&file).await.unwrap();
    let sessions = SessionService::new(SqliteSessionRepository::new(pool.clone()));
    let session = sessions
        .create(CreateSession {
            title: "Voice".to_owned(),
            mode: SessionMode::LiveInterview,
            context: serde_json::json!({}),
            configuration: serde_json::json!({ "tts": "enabled" }),
        })
        .await
        .unwrap();

    // A fixed resolver stands in for a configured remote adapter.
    let voice = VoiceService::with_tts(
        sessions.clone(),
        TimelineService::new(SqliteTimelineRepository::new(pool)),
        Arc::new(FixedTtsResolver::new(Arc::new(EmbeddedTts))),
    );
    let spoken = voice
        .speak(
            session.id,
            SpeakRequest {
                text: "Feedback".to_owned(),
            },
        )
        .await
        .unwrap();
    assert!(spoken.enabled);
    assert_eq!(spoken.content_type.as_deref(), Some("audio/wav"));
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(spoken.audio_base64.unwrap())
        .unwrap();
    assert!(bytes.starts_with(b"RIFF"));

    // Disabling TTS in configuration still short-circuits before the provider.
    sessions
        .update(
            session.id,
            UpdateSession {
                configuration: Some(serde_json::json!({ "tts": "disabled" })),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let silent = voice
        .speak(
            session.id,
            SpeakRequest {
                text: "Feedback".to_owned(),
            },
        )
        .await
        .unwrap();
    assert!(!silent.enabled);
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn saved_configuration_round_trips_through_the_service() {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join(format!("tts-{}.sqlite", Uuid::now_v7()));
    let pool = talkcer_core::connect_and_migrate(&file).await.unwrap();
    let service = talkcer_core::AiConfigurationService::new(
        talkcer_core::SqliteAiConfigurationRepository::new(pool),
    );

    let saved = service
        .save(SaveAiConfiguration {
            id: None,
            name: "local".to_owned(),
            stt: config(ProviderKind::Embedded, "embedded-stt", None, false),
            thinker: config(ProviderKind::Embedded, "embedded-thinker", None, false),
            tts: config(
                ProviderKind::SelfHosted,
                "self-hosted-tts",
                Some("http://127.0.0.1:9000/v1"),
                true,
            ),
        })
        .await
        .unwrap();
    let result = service
        .test(saved.id, talkcer_core::Capability::Tts)
        .await
        .unwrap();
    assert!(result.ok, "{}", result.message);

    let listed = service.list().await.unwrap();
    let selection = select_tts(
        &session_with_tts(serde_json::json!(saved.id.to_string())),
        &listed,
    )
    .unwrap();
    assert_eq!(selection.provider, "self-hosted-tts");
    let _ = std::fs::remove_file(file);
}
