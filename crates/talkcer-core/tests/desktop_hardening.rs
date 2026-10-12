//! Overlay persistence, session isolation, export, timeout, and rate limiting.

use std::path::PathBuf;
use std::time::Duration;

use talkcer_core::{
    accept_assistant_result, connect_and_migrate, export_session, run_with_timeout, CreateSession,
    ExportFormat, OverlayService, OverlayState, RateGate, SessionMode, SessionService, Source,
    Speaker, SqliteSessionRepository, SqliteTimelineRepository, TimelineService, UpsertTranscript,
};
use uuid::Uuid;

fn path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("{name}-{}.sqlite", Uuid::now_v7()))
}

#[tokio::test]
async fn overlay_preferences_persist_and_isolate_sessions() {
    let file = path("overlay");
    let pool = connect_and_migrate(&file).await.unwrap();
    let sessions = SessionService::new(SqliteSessionRepository::new(pool.clone()));
    let timeline = TimelineService::new(SqliteTimelineRepository::new(pool.clone()));
    let overlay = OverlayService::new(pool, timeline.clone());

    let first = sessions.create(CreateSession { title: "A".into(), mode: SessionMode::LiveInterview, context: serde_json::json!({}), configuration: serde_json::json!({}) }).await.unwrap();
    let second = sessions.create(CreateSession { title: "B".into(), mode: SessionMode::LiveInterview, context: serde_json::json!({}), configuration: serde_json::json!({}) }).await.unwrap();
    timeline.upsert_transcript(first.id, line(Speaker::Remote, "from a")).await.unwrap();
    timeline.upsert_transcript(second.id, line(Speaker::Ai, "from b")).await.unwrap();

    assert_eq!(overlay.get().await.unwrap().state, OverlayState::Hidden);
    let shown = overlay.show().await.unwrap();
    assert_eq!(shown.state, OverlayState::Visible);
    assert!(shown.always_on_top);
    let moved = overlay.move_to(talkcer_core::OverlayPosition { x: 80, y: 120 }).await.unwrap();
    assert_eq!((moved.position_x, moved.position_y), (80, 120));
    assert!((overlay.set_opacity(0.4).await.unwrap().opacity - 0.4).abs() < f64::EPSILON);
    assert_eq!(overlay.set_click_through(true).await.unwrap().state, OverlayState::ClickThrough);
    overlay.follow(first.id).await.unwrap();
    let first_view = overlay.view().await.unwrap();
    assert_eq!(first_view.items.len(), 1);
    assert!(first_view.items[0].text.contains("from a"));

    overlay.follow(second.id).await.unwrap();
    let second_view = overlay.view().await.unwrap();
    assert!(second_view.items.iter().all(|item| item.session_id == second.id));
    assert!(second_view.items.iter().all(|item| !item.text.contains("from a")));
    assert!(!accept_assistant_result(second.id, first.id));

    let hidden = overlay.hide().await.unwrap();
    assert_eq!(hidden.state, OverlayState::Hidden);
    assert!(overlay.view().await.unwrap().items.is_empty());
    assert_eq!(overlay.get().await.unwrap().active_session_id, Some(second.id));
    // Undetectable mode persists independently of the overlay visibility.
    assert!(!overlay.get().await.unwrap().undetectable);
    assert!(overlay.set_undetectable(true).await.unwrap().undetectable);
    assert!(overlay.get().await.unwrap().undetectable);
    assert!(!overlay.set_undetectable(false).await.unwrap().undetectable);
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn export_is_readable_and_limits_stop_late_work() {
    let file = path("export");
    let pool = connect_and_migrate(&file).await.unwrap();
    let sessions = SessionService::new(SqliteSessionRepository::new(pool.clone()));
    let timeline = TimelineService::new(SqliteTimelineRepository::new(pool));
    let session = sessions.create(CreateSession { title: "Notes".into(), mode: SessionMode::Learning, context: serde_json::json!({}), configuration: serde_json::json!({}) }).await.unwrap();
    timeline.upsert_transcript(session.id, line(Speaker::LocalUser, "What is backoff?")).await.unwrap();

    let json = export_session(&sessions, &timeline, session.id, ExportFormat::Json).await.unwrap();
    assert!(json.body.contains("What is backoff?"));
    let markdown = export_session(&sessions, &timeline, session.id, ExportFormat::Markdown).await.unwrap();
    assert!(markdown.body.starts_with("# Notes"));
    assert!(markdown.body.contains("local_user: What is backoff?"));

    let timed_out = run_with_timeout(Duration::from_millis(10), async {
        tokio::time::sleep(Duration::from_millis(50)).await;
        Ok::<(), talkcer_core::CoreError>(())
    })
    .await;
    assert!(matches!(timed_out, Err(talkcer_core::CoreError::ProviderTimeout)));

    let mut gate = RateGate::new(2);
    assert!(gate.allow("token").is_ok());
    assert!(gate.allow("token").is_ok());
    assert!(matches!(gate.allow("token"), Err(talkcer_core::CoreError::RateLimited)));
    let _ = std::fs::remove_file(file);
}

fn line(speaker: Speaker, text: &str) -> UpsertTranscript {
    UpsertTranscript {
        utterance_id: None,
        speaker,
        source: Source::Text,
        text: text.to_owned(),
        started_at_ms: 1_000,
        ended_at_ms: None,
        is_final: true,
        metadata: None,
    }
}
