use std::sync::Arc;

use talkcer_core::{
    connect_and_migrate, CaptureAction, CaptureFixture, CaptureService, CaptureState, CreateSession,
    FixtureDeviceProvider, RemoteAccessService, SessionMode, SessionService, SqliteSessionRepository,
    SqliteTimelineRepository, TimelineQuery, TimelineService,
};
use uuid::Uuid;

fn path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("{name}-{}.sqlite", Uuid::now_v7()))
}

fn fixture_capture(
    pool: &sqlx::SqlitePool,
) -> CaptureService<SqliteSessionRepository, SqliteTimelineRepository> {
    CaptureService::with_provider(
        SessionService::new(SqliteSessionRepository::new(pool.clone())),
        TimelineService::new(SqliteTimelineRepository::new(pool.clone())),
        pool.clone(),
        Arc::new(FixtureDeviceProvider::default_pair()),
    )
}

#[tokio::test]
async fn windows_capture_keeps_channels_separate_and_recovers() {
    let file = path("capture");
    let pool = connect_and_migrate(&file).await.unwrap();
    let sessions = SessionService::new(SqliteSessionRepository::new(pool.clone()));
    let session = sessions
        .create(CreateSession {
            title: "Live".to_owned(),
            mode: SessionMode::LiveInterview,
            context: serde_json::json!({}),
            configuration: serde_json::json!({}),
        })
        .await
        .unwrap();
    let capture = fixture_capture(&pool);
    let started = capture.control(session.id, CaptureAction::Start).await.unwrap();
    assert_eq!(started.state, CaptureState::Capturing);
    assert_eq!(started.devices.len(), 2);
    assert_eq!(started.devices[0].channel, "microphone");
    assert_eq!(started.devices[1].channel, "system_audio");
    assert_eq!(started.devices[1].direction, "render_loopback");
    let local = capture
        .inject(session.id, CaptureFixture {
            channel: "microphone".to_owned(),
            utterance_id: None,
            text: "I".to_owned(),
            started_at_ms: 10,
            is_final: false,
        })
        .await
        .unwrap();
    let local_final = capture
        .inject(session.id, CaptureFixture {
            channel: "microphone".to_owned(),
            utterance_id: Some(local.item.id),
            text: "I can hear you.".to_owned(),
            started_at_ms: 10,
            is_final: true,
        })
        .await
        .unwrap();
    let remote = capture
        .inject(session.id, CaptureFixture {
            channel: "system_audio".to_owned(),
            utterance_id: None,
            text: "Tell me about retries.".to_owned(),
            started_at_ms: 20,
            is_final: true,
        })
        .await
        .unwrap();
    assert_ne!(local_final.item.id, remote.item.id);
    assert_eq!(local_final.item.speaker.as_str(), "local_user");
    assert_eq!(remote.item.speaker.as_str(), "remote");

    assert_eq!(capture.control(session.id, CaptureAction::Pause).await.unwrap().state, CaptureState::Paused);
    assert!(capture.inject(session.id, CaptureFixture {
        channel: "microphone".to_owned(), utterance_id: None, text: "late".to_owned(), started_at_ms: 30, is_final: true,
    }).await.is_err());
    assert_eq!(capture.control(session.id, CaptureAction::Resume).await.unwrap().state, CaptureState::Capturing);
    let error = capture.control(session.id, CaptureAction::Disconnect).await.unwrap();
    assert_eq!(error.state, CaptureState::Error);
    assert_eq!(capture.control(session.id, CaptureAction::Recover).await.unwrap().state, CaptureState::Idle);
    assert_eq!(capture.control(session.id, CaptureAction::Stop).await.unwrap().state, CaptureState::Idle);

    let reopened = CaptureService::new(
        SessionService::new(SqliteSessionRepository::new(pool.clone())),
        TimelineService::new(SqliteTimelineRepository::new(pool.clone())),
        pool,
    );
    assert_eq!(reopened.status(session.id).await.unwrap().platform, "windows");
    let page = TimelineService::new(SqliteTimelineRepository::new(connect_and_migrate(&file).await.unwrap()))
        .list(session.id, TimelineQuery { limit: 100, before: None })
        .await
        .unwrap();
    assert_eq!(page.items.len(), 2);
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn remote_access_rejects_until_pairing_matches() {
    let file = path("remote");
    let service = RemoteAccessService::new(connect_and_migrate(&file).await.unwrap());
    assert!(!service.status().await.unwrap().enabled);
    assert!(service.authorize("anything").await.is_err());
    let paired = service.pair(true).await.unwrap();
    let token = paired.token.unwrap();
    assert!(service.authorize(&token).await.is_ok());
    assert!(service.authorize("wrong").await.is_err());
    let disabled = service.pair(false).await.unwrap();
    assert!(!disabled.enabled);
    assert!(service.authorize(&token).await.is_err());
    let _ = std::fs::remove_file(file);
}
