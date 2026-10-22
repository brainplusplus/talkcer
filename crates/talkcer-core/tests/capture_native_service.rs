//! Service-level test for the real WASAPI device provider.
//!
//! This exercises the wiring between `CaptureService` and the native reader
//! threads: `start` opens real endpoints through `spawn_blocking`, and `status`
//! reports the live frame counts. On a machine without audio hardware the typed
//! device error is the accepted outcome.

use std::time::{Duration, Instant};

use talkcer_core::{
    connect_and_migrate, CaptureAction, CaptureService, CaptureState, CoreError, CreateSession,
    SessionMode, SessionService, SqliteSessionRepository, SqliteTimelineRepository,
    TimelineService,
};
use uuid::Uuid;

fn path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("{name}-{}.sqlite", Uuid::now_v7()))
}

#[tokio::test]
async fn service_start_opens_native_devices_and_reports_frames() {
    let file = path("native-service");
    let pool = connect_and_migrate(&file).await.unwrap();
    let sessions = SessionService::new(SqliteSessionRepository::new(pool.clone()));
    let session = sessions
        .create(CreateSession {
            title: "Native".to_owned(),
            mode: SessionMode::LiveInterview,
            context: serde_json::json!({}),
            configuration: serde_json::json!({}),
        })
        .await
        .unwrap();
    let capture = CaptureService::new(
        sessions,
        TimelineService::new(SqliteTimelineRepository::new(pool.clone())),
        pool,
    );

    let started = match capture.control(session.id, CaptureAction::Start).await {
        Ok(status) => status,
        Err(CoreError::AudioDevice(message)) => {
            assert!(
                !message.trim().is_empty(),
                "device error must carry a message"
            );
            let _ = std::fs::remove_file(file);
            return;
        }
        Err(other) => panic!("start returned an unexpected error: {other}"),
    };

    assert_eq!(started.state, CaptureState::Capturing);
    assert_eq!(
        started.devices.len(),
        2,
        "expected microphone and system audio"
    );
    let channels: Vec<&str> = started
        .devices
        .iter()
        .map(|device| device.channel.as_str())
        .collect();
    assert!(channels.contains(&"microphone"), "{channels:?}");
    assert!(channels.contains(&"system_audio"), "{channels:?}");

    // The microphone endpoint should advance; a silent system endpoint may not.
    let deadline = Instant::now() + Duration::from_millis(400);
    let mut frames = 0;
    while Instant::now() < deadline {
        let status = capture.status(session.id).await.unwrap();
        frames = status
            .devices
            .iter()
            .find(|device| device.channel == "microphone")
            .map_or(0, |device| device.frames);
        if frames > 0 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(frames > 0, "service never reported microphone frames");

    // Stop must tear the readers down and return to idle.
    let stopped = capture
        .control(session.id, CaptureAction::Stop)
        .await
        .unwrap();
    assert_eq!(stopped.state, CaptureState::Idle);
    let _ = std::fs::remove_file(file);
}
