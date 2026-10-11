use base64::Engine;
use talkcer_core::{
    connect_and_migrate, CreateSession, SessionMode, SessionService, Source, Speaker, SpeakRequest,
    SqliteSessionRepository, SqliteTimelineRepository, TimelineService, UpdateSession, VoiceAudio, VoiceService,
};
use uuid::Uuid;

fn path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("voice-{name}-{}.sqlite", Uuid::now_v7()))
}

fn encode(text: &str) -> String {
    base64::engine::general_purpose::STANDARD.encode(text.as_bytes())
}

#[tokio::test]
async fn mock_audio_updates_one_transcript_and_tts_follows_configuration() {
    let file = path("stream");
    let pool = connect_and_migrate(&file).await.unwrap();
    let sessions = SessionService::new(SqliteSessionRepository::new(pool.clone()));
    let session = sessions
        .create(CreateSession {
            title: "Voice".to_owned(),
            mode: SessionMode::Learning,
            context: serde_json::json!({}),
            configuration: serde_json::json!({ "tts": "disabled" }),
        })
        .await
        .unwrap();
    let voice = VoiceService::new(sessions.clone(), TimelineService::new(SqliteTimelineRepository::new(pool)));

    let partial = voice
        .transcribe(
            session.id,
            VoiceAudio {
                utterance_id: None,
                audio_source: "browser_mic".to_owned(),
                text: None,
                chunks: vec![encode("How")],
                started_at_ms: 1000,
                is_final: false,
            },
        )
        .await
        .unwrap();
    let id = partial.item.id;
    let updated = voice
        .transcribe(
            session.id,
            VoiceAudio {
                utterance_id: Some(id),
                audio_source: "mock_audio".to_owned(),
                text: Some("How does an index help?".to_owned()),
                chunks: Vec::new(),
                started_at_ms: 1000,
                is_final: true,
            },
        )
        .await
        .unwrap();
    assert!(!partial.created || updated.created == false);
    assert_eq!(updated.item.id, id);
    assert_eq!(updated.item.speaker, Speaker::LocalUser);
    assert_eq!(updated.item.source, Source::Microphone);
    assert_eq!(updated.item.metadata["audio_source"], "browser_mic");
    assert!(updated.item.is_final);

    let silent = voice
        .speak(session.id, SpeakRequest { text: "Feedback".to_owned() })
        .await
        .unwrap();
    assert!(!silent.enabled);

    sessions
        .update(
            session.id,
            UpdateSession {
                configuration: Some(serde_json::json!({ "tts": "enabled" })),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let spoken = voice
        .speak(session.id, SpeakRequest { text: "Feedback".to_owned() })
        .await
        .unwrap();
    assert!(spoken.enabled);
    assert_eq!(spoken.content_type.as_deref(), Some("audio/wav"));
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(spoken.audio_base64.unwrap())
        .unwrap();
    assert!(bytes.starts_with(b"RIFF"));
    let _ = std::fs::remove_file(file);
}
