//! Screen capture into the running mode: direct answers and staged context.

use std::sync::Arc;

use talkcer_core::{
    answer_from_screen, connect_and_migrate, is_pending, mark_sent, pending_screens, record_screen,
    screen_task, CreateSession, EmbeddedThinker, ItemKind, ScreenCapturer, ScreenFrame,
    ScreenIntent, SessionMode, SessionService, Speaker, SqliteSessionRepository,
    SqliteTimelineRepository, TimelineItem, TimelineQuery, TimelineService, VisionReader,
};
use uuid::Uuid;

fn path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("{name}-{}.sqlite", Uuid::now_v7()))
}

struct FixedCapturer;

#[async_trait::async_trait]
impl ScreenCapturer for FixedCapturer {
    async fn capture(&self) -> Result<ScreenFrame, talkcer_core::CoreError> {
        Ok(ScreenFrame { png: vec![1, 2, 3], width: 1920, height: 1080 })
    }
}

struct FixedReader;

#[async_trait::async_trait]
impl VisionReader for FixedReader {
    async fn read(&self, _frame: &ScreenFrame) -> Result<String, talkcer_core::CoreError> {
        Ok("Question: explain ownership in Rust.".to_owned())
    }
}

async fn service() -> (TimelineService<SqliteTimelineRepository>, Uuid) {
    let pool = connect_and_migrate(&path("screen")).await.unwrap();
    let sessions = SessionService::new(SqliteSessionRepository::new(pool.clone()));
    let session = sessions
        .create(CreateSession {
            title: "Screen".into(),
            mode: SessionMode::LiveInterview,
            context: serde_json::json!({}),
            configuration: serde_json::json!({}),
        })
        .await
        .unwrap();
    (TimelineService::new(SqliteTimelineRepository::new(pool)), session.id)
}

#[tokio::test]
async fn direct_capture_answers_in_the_session_mode() {
    let thinker: Arc<dyn talkcer_core::Thinker> = Arc::new(EmbeddedThinker);
    let frame = FixedCapturer.capture().await.unwrap();
    let reading = FixedReader.read(&frame).await.unwrap();

    let live = answer_from_screen(&thinker, SessionMode::LiveInterview, &reading, None)
        .await
        .unwrap();
    assert!(live.reply.text.contains("Answer the question it asks"));
    assert!(live.reply.text.contains("explain ownership"));

    let mock = answer_from_screen(&thinker, SessionMode::MockInterview, &reading, None)
        .await
        .unwrap();
    assert!(mock.reply.text.contains("mock-interview question"));

    let learning = answer_from_screen(
        &thinker,
        SessionMode::Learning,
        &reading,
        Some("focus on lifetimes"),
    )
    .await
    .unwrap();
    assert!(learning.reply.text.contains("Explain what it shows"));
    assert!(learning.reply.text.contains("focus on lifetimes"));
}

#[tokio::test]
async fn staged_captures_wait_until_sent() {
    let (timeline, session_id) = service().await;
    let frame = FixedCapturer.capture().await.unwrap();

    record_screen(&timeline, session_id, ScreenIntent::Stage, &frame, None, None)
        .await
        .unwrap();
    record_screen(&timeline, session_id, ScreenIntent::Stage, &frame, None, None)
        .await
        .unwrap();

    let page = timeline
        .list(session_id, TimelineQuery { limit: 20, before: None })
        .await
        .unwrap();
    let waiting = pending_screens(&page.items);
    assert_eq!(waiting.len(), 2);
    assert!(waiting.iter().all(|item| item.kind == ItemKind::SystemEvent));

    // Sending clears the flag, so a second send does not repeat them.
    for item in waiting {
        let mut metadata = item.metadata.clone();
        mark_sent(&mut metadata);
        timeline.set_metadata(item.id, metadata).await.unwrap();
    }
    let after = timeline
        .list(session_id, TimelineQuery { limit: 20, before: None })
        .await
        .unwrap();
    assert!(after.items.iter().all(|item| !is_pending(item)));
}

#[tokio::test]
async fn direct_capture_stores_the_answer_beside_the_screen() {
    let (timeline, session_id) = service().await;
    let frame = FixedCapturer.capture().await.unwrap();
    let outcome = record_screen(
        &timeline,
        session_id,
        ScreenIntent::Direct,
        &frame,
        Some("Question: explain ownership."),
        Some("Ownership means one value has one owner."),
    )
    .await
    .unwrap();

    assert!(!is_pending(&outcome.captured));
    let reply = outcome.reply.expect("a direct capture answers");
    assert_eq!(reply.speaker, Speaker::Ai);
    assert_eq!(reply.kind, ItemKind::AiMessage);

    let page = timeline
        .list(session_id, TimelineQuery { limit: 20, before: None })
        .await
        .unwrap();
    assert_eq!(page.items.len(), 2);
}

#[test]
fn each_mode_phrases_the_screen_prompt_differently() {
    let reading = "a diagram of two threads";
    let prompts = [
        SessionMode::LiveInterview,
        SessionMode::MockInterview,
        SessionMode::Learning,
    ]
    .map(|mode| screen_task(mode, reading, None).prompt());
    assert!(prompts.iter().all(|prompt| prompt.contains(reading)));
    assert_eq!(prompts.iter().collect::<std::collections::HashSet<_>>().len(), 3);
}

fn _typed(item: TimelineItem) -> TimelineItem {
    item
}
