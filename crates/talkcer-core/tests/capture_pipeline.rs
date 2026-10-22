//! Pipeline tests for scripted audio.
//!
//! [`ScriptedDeviceProvider`] feeds deterministic PCM through the real service
//! path, so these tests cover segmentation, transcription, channel-to-speaker
//! routing, and automatic disconnect detection without touching audio hardware.

use std::sync::Arc;

use talkcer_core::{
    connect_and_migrate, utf8_pcm, CaptureAction, CaptureDevice, CaptureService, CaptureState,
    CreateSession, EmbeddedTranscriber, FixedSttResolver, ScriptedDeviceProvider, SessionMode,
    SessionService, SqliteSessionRepository, SqliteTimelineRepository, TimelineQuery,
    TimelineService,
};
use uuid::Uuid;

fn path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("{name}-{}.sqlite", Uuid::now_v7()))
}

fn device(channel: &str, pcm: Vec<u8>) -> (CaptureDevice, Vec<u8>) {
    (
        CaptureDevice {
            channel: channel.to_owned(),
            id: format!("Scripted {channel}"),
            name: format!("Scripted {channel}"),
            direction: if channel == "system_audio" {
                "render_loopback"
            } else {
                "capture"
            }
            .to_owned(),
            state: "active".to_owned(),
            sample_rate: 48_000,
            channels: 2,
            frames: 0,
        },
        pcm,
    )
}

#[tokio::test]
async fn pipeline_routes_each_channel_to_its_speaker() {
    let file = path("pipeline");
    let pool = connect_and_migrate(&file).await.unwrap();
    let sessions = SessionService::new(SqliteSessionRepository::new(pool.clone()));
    let timeline = TimelineService::new(SqliteTimelineRepository::new(pool.clone()));
    let session = sessions
        .create(CreateSession {
            title: "Live".to_owned(),
            mode: SessionMode::LiveInterview,
            context: serde_json::json!({}),
            configuration: serde_json::json!({}),
        })
        .await
        .unwrap();

    let provider = ScriptedDeviceProvider::spoken_pair("I can hear you.", "Tell me about retries.");
    let capture =
        CaptureService::with_provider(sessions, timeline.clone(), pool, Arc::new(provider));

    let started = capture
        .control(session.id, CaptureAction::Start)
        .await
        .unwrap();
    assert_eq!(started.state, CaptureState::Capturing);

    let tick = capture.poll(session.id).await.unwrap();
    assert!(tick.disconnected.is_none());
    assert_eq!(tick.transcripts.len(), 2, "{:?}", tick.transcripts);

    let page = timeline
        .list(
            session.id,
            TimelineQuery {
                limit: 100,
                before: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items.len(), 2);

    let local = page
        .items
        .iter()
        .find(|item| item.speaker.as_str() == "local_user")
        .expect("local transcript");
    assert_eq!(local.text, "I can hear you.");
    assert_eq!(local.source.as_str(), "microphone");
    assert_eq!(local.metadata["audio_channel"], "microphone");

    let remote = page
        .items
        .iter()
        .find(|item| item.speaker.as_str() == "remote")
        .expect("remote transcript");
    assert_eq!(remote.text, "Tell me about retries.");
    assert_eq!(remote.source.as_str(), "system_audio");
    assert_eq!(remote.metadata["audio_channel"], "system_audio");

    // A second poll has nothing new to store.
    assert!(capture
        .poll(session.id)
        .await
        .unwrap()
        .transcripts
        .is_empty());

    assert_eq!(
        capture
            .control(session.id, CaptureAction::Stop)
            .await
            .unwrap()
            .state,
        CaptureState::Idle
    );
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn pipeline_records_the_resolved_stt_provider() {
    let file = path("stt-provider");
    let pool = connect_and_migrate(&file).await.unwrap();
    let sessions = SessionService::new(SqliteSessionRepository::new(pool.clone()));
    let timeline = TimelineService::new(SqliteTimelineRepository::new(pool.clone()));
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

    let provider = ScriptedDeviceProvider::spoken_pair("hello there", "general kenobi");
    let capture = CaptureService::with_resolver(
        sessions,
        timeline.clone(),
        pool,
        Arc::new(provider),
        Arc::new(FixedSttResolver::new(Arc::new(EmbeddedTranscriber))),
    );

    capture
        .control(session.id, CaptureAction::Start)
        .await
        .unwrap();
    let tick = capture.poll(session.id).await.unwrap();
    assert_eq!(tick.transcripts.len(), 2, "{:?}", tick.transcripts);
    for saved in &tick.transcripts {
        assert_eq!(saved.item.metadata["stt_provider"], "embedded-stt");
    }

    let page = timeline
        .list(
            session.id,
            TimelineQuery {
                limit: 100,
                before: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(page.items.len(), 2);
    assert_eq!(page.items[0].metadata["stt_provider"], "embedded-stt");
    let _ = std::fs::remove_file(file);
}

/// A transcriber that always answers the same text, to prove the pipeline calls
/// whatever the resolver returns rather than the built-in adapter.
struct CannedTranscriber;

#[async_trait::async_trait]
impl talkcer_core::Transcriber for CannedTranscriber {
    async fn transcribe(
        &self,
        _segment: &talkcer_core::Segment,
    ) -> Result<Option<String>, talkcer_core::CoreError> {
        Ok(Some("canned".to_owned()))
    }

    fn provider(&self) -> &'static str {
        "canned-stt"
    }
}

#[tokio::test]
async fn pipeline_uses_the_resolver_transcriber_not_the_default() {
    let file = path("canned");
    let pool = connect_and_migrate(&file).await.unwrap();
    let sessions = SessionService::new(SqliteSessionRepository::new(pool.clone()));
    let timeline = TimelineService::new(SqliteTimelineRepository::new(pool.clone()));
    let session = sessions
        .create(CreateSession {
            title: "Live".to_owned(),
            mode: SessionMode::LiveInterview,
            context: serde_json::json!({}),
            configuration: serde_json::json!({}),
        })
        .await
        .unwrap();

    let provider = ScriptedDeviceProvider::spoken_pair("ignored", "ignored");
    let capture = CaptureService::with_resolver(
        sessions,
        timeline.clone(),
        pool,
        Arc::new(provider),
        Arc::new(FixedSttResolver::new(Arc::new(CannedTranscriber))),
    );

    capture
        .control(session.id, CaptureAction::Start)
        .await
        .unwrap();
    let tick = capture.poll(session.id).await.unwrap();
    assert_eq!(tick.transcripts.len(), 2);
    for saved in &tick.transcripts {
        assert_eq!(saved.item.text, "canned");
        assert_eq!(saved.item.metadata["stt_provider"], "canned-stt");
    }
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn silent_channel_produces_no_transcript() {
    let file = path("silent");
    let pool = connect_and_migrate(&file).await.unwrap();
    let sessions = SessionService::new(SqliteSessionRepository::new(pool.clone()));
    let timeline = TimelineService::new(SqliteTimelineRepository::new(pool.clone()));
    let session = sessions
        .create(CreateSession {
            title: "Live".to_owned(),
            mode: SessionMode::LiveInterview,
            context: serde_json::json!({}),
            configuration: serde_json::json!({}),
        })
        .await
        .unwrap();

    // A full second of stereo silence at 48 kHz.
    let provider = ScriptedDeviceProvider::new(vec![
        device("microphone", vec![0_u8; 48_000 * 4]),
        device("system_audio", vec![0_u8; 48_000 * 4]),
    ]);
    let capture =
        CaptureService::with_provider(sessions, timeline.clone(), pool, Arc::new(provider));

    capture
        .control(session.id, CaptureAction::Start)
        .await
        .unwrap();
    let tick = capture.poll(session.id).await.unwrap();
    assert!(
        tick.transcripts.is_empty(),
        "silence must not create a transcript"
    );
    let page = timeline
        .list(
            session.id,
            TimelineQuery {
                limit: 100,
                before: None,
            },
        )
        .await
        .unwrap();
    assert!(page.items.is_empty());
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn reader_error_moves_capture_to_error_and_recovers() {
    let file = path("disconnect");
    let pool = connect_and_migrate(&file).await.unwrap();
    let sessions = SessionService::new(SqliteSessionRepository::new(pool.clone()));
    let timeline = TimelineService::new(SqliteTimelineRepository::new(pool.clone()));
    let session = sessions
        .create(CreateSession {
            title: "Live".to_owned(),
            mode: SessionMode::LiveInterview,
            context: serde_json::json!({}),
            configuration: serde_json::json!({}),
        })
        .await
        .unwrap();

    let provider = ScriptedDeviceProvider::spoken_pair("hello", "world");
    let handle = provider.clone();
    let capture = CaptureService::with_provider(sessions, timeline, pool, Arc::new(provider));

    capture
        .control(session.id, CaptureAction::Start)
        .await
        .unwrap();
    assert_eq!(capture.live_sessions(), vec![session.id]);

    // Simulate the endpoint being unplugged mid-call.
    handle
        .stream("system_audio")
        .unwrap()
        .fail("device unplugged");
    let tick = capture.poll(session.id).await.unwrap();
    let message = tick
        .disconnected
        .expect("a reader failure must be reported");
    assert!(message.contains("system_audio"), "{message}");

    let status = capture.status(session.id).await.unwrap();
    assert_eq!(status.state, CaptureState::Error);
    assert!(status.error.unwrap().contains("system_audio"));
    assert!(
        capture.live_sessions().is_empty(),
        "readers must stop on disconnect"
    );

    assert_eq!(
        capture
            .control(session.id, CaptureAction::Recover)
            .await
            .unwrap()
            .state,
        CaptureState::Idle
    );
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn utf8_pcm_round_trips_through_the_segmenter() {
    // Guard the encoder the scripted provider relies on.
    let pcm = utf8_pcm("ok");
    let segmenter = talkcer_core::Segmenter::for_speech(48_000, 2);
    let segments = segmenter.segment(1_000, &pcm);
    assert_eq!(segments.len(), 1);
    assert_eq!(
        talkcer_core::decode_text(&segments[0].bytes).as_deref(),
        Some("ok")
    );
}
