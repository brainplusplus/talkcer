//! Thinker wiring through the services.
//!
//! A canned thinker stands in for a real model so these tests verify the seam,
//! not the network: assistant actions, mock interviews, and learning turns all
//! take their text from the session's configured thinker.

use std::sync::Arc;

use talkcer_core::{
    connect_and_migrate, stub_assistant_text, AssistantAction, AssistantActionRequest,
    CreateSession, ItemKind, LearningAction, LearningService, LearningTurn, MockAction,
    MockInterviewService, MockTurn, ProviderKind, SessionMode, SessionService, SessionStatus,
    SqliteSessionRepository, SqliteTimelineRepository, Thinker, ThinkerReply, ThinkerResolver,
    ThinkerTask, TimelineQuery, TimelineService, UpsertTranscript,
};
use uuid::Uuid;

/// A thinker that labels every reply with the task it answered.
struct CannedThinker;

#[async_trait::async_trait]
impl Thinker for CannedThinker {
    async fn complete(&self, task: &ThinkerTask) -> Result<ThinkerReply, talkcer_core::CoreError> {
        Ok(ThinkerReply {
            text: format!("canned:{}", task.label()),
            provider: "canned-thinker".to_owned(),
        })
    }

    fn provider(&self) -> &'static str {
        "canned-thinker"
    }
}

fn canned_timeline(pool: sqlx::SqlitePool) -> TimelineService<SqliteTimelineRepository> {
    let resolver: Arc<dyn ThinkerResolver> = Arc::new(talkcer_core::FixedThinkerResolver::new(
        Arc::new(CannedThinker),
    ));
    TimelineService::with_thinker(SqliteTimelineRepository::new(pool), resolver)
}

fn path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("thinker-pipeline-{name}-{}.sqlite", Uuid::now_v7()))
}

fn partial(text: &str) -> UpsertTranscript {
    UpsertTranscript {
        utterance_id: None,
        speaker: talkcer_core::Speaker::Remote,
        source: talkcer_core::Source::SystemAudio,
        text: text.to_owned(),
        started_at_ms: 1_000,
        ended_at_ms: Some(2_000),
        is_final: true,
        metadata: None,
    }
}

#[tokio::test]
async fn assistant_action_uses_the_session_thinker_and_records_metadata() {
    let file = path("assistant");
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

    // The service resolves the session thinker through the attached lookup.
    let timeline = canned_timeline(pool).with_sessions(sessions.clone());
    let transcript = timeline
        .upsert_transcript(session.id, partial("retries"))
        .await
        .unwrap();

    let result = timeline
        .assistant_action(
            session.id,
            AssistantActionRequest {
                action: AssistantAction::Explain,
                target_id: transcript.item.id,
                language: Some("id".to_owned()),
            },
        )
        .await
        .unwrap();

    assert_eq!(result.message.kind, ItemKind::AiMessage);
    assert_eq!(result.message.text, "canned:assistant_action");
    assert_eq!(result.message.metadata["provider"], "canned-thinker");
    assert_eq!(result.message.metadata["task"], "assistant_action");
    assert_eq!(result.message.metadata["stub"], false);
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn assistant_action_without_a_session_source_stays_offline() {
    let file = path("assistant-offline");
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
    let timeline = TimelineService::new(SqliteTimelineRepository::new(pool));
    let transcript = timeline
        .upsert_transcript(session.id, partial("retries"))
        .await
        .unwrap();

    let result = timeline
        .assistant_action(
            session.id,
            AssistantActionRequest {
                action: AssistantAction::Explain,
                target_id: transcript.item.id,
                language: None,
            },
        )
        .await
        .unwrap();

    assert_eq!(
        result.message.text,
        stub_assistant_text(AssistantAction::Explain, transcript.item.id)
    );
    assert_eq!(result.message.metadata["provider"], "embedded-thinker");
    assert_eq!(result.message.metadata["stub"], true);
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn mock_interview_takes_every_reply_from_the_session_thinker() {
    let file = path("mock");
    let pool = connect_and_migrate(&file).await.unwrap();
    let sessions = SessionService::new(SqliteSessionRepository::new(pool.clone()));
    let session = sessions
        .create(CreateSession {
            title: "Backend mock".to_owned(),
            mode: SessionMode::MockInterview,
            context: serde_json::json!({ "role": "backend engineer" }),
            configuration: serde_json::json!({}),
        })
        .await
        .unwrap();

    let controller = MockInterviewService::new(sessions.clone(), canned_timeline(pool));
    let started = controller
        .turn(
            session.id,
            MockTurn {
                action: MockAction::Start,
                text: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(started.status, SessionStatus::Active);
    assert_eq!(started.items[0].text, "canned:mock_question");

    let answered = controller
        .turn(
            session.id,
            MockTurn {
                action: MockAction::Answer,
                text: Some("I separated retries.".to_owned()),
            },
        )
        .await
        .unwrap();
    assert_eq!(answered.items[1].text, "canned:mock_follow_up");

    let completed = controller
        .turn(
            session.id,
            MockTurn {
                action: MockAction::Complete,
                text: Some("Bounded retries.".to_owned()),
            },
        )
        .await
        .unwrap();
    assert_eq!(completed.status, SessionStatus::Completed);
    assert_eq!(completed.items[1].text, "canned:mock_feedback");
    assert_eq!(completed.items[2].text, "canned:mock_summary");
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn learning_turns_take_every_reply_from_the_session_thinker() {
    let file = path("learning");
    let pool = connect_and_migrate(&file).await.unwrap();
    let sessions = SessionService::new(SqliteSessionRepository::new(pool.clone()));
    let session = sessions
        .create(CreateSession {
            title: "SQL indexes".to_owned(),
            mode: SessionMode::Learning,
            context: serde_json::json!({ "topic": "database indexes" }),
            configuration: serde_json::json!({}),
        })
        .await
        .unwrap();

    let controller = LearningService::new(sessions.clone(), canned_timeline(pool));
    let started = controller
        .turn(
            session.id,
            LearningTurn {
                action: LearningAction::Start,
                text: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(started.items[0].text, "canned:learning_intro");
    assert_eq!(started.items[1].text, "canned:learning_start_practice");

    let discussed = controller
        .turn(
            session.id,
            LearningTurn {
                action: LearningAction::Discuss,
                text: Some("When does the planner skip an index?".to_owned()),
            },
        )
        .await
        .unwrap();
    assert_eq!(discussed.items[1].text, "canned:learning_answer");
    assert_eq!(discussed.items[2].text, "canned:learning_practice");

    let answered = controller
        .turn(
            session.id,
            LearningTurn {
                action: LearningAction::Answer,
                text: Some("Low selectivity.".to_owned()),
            },
        )
        .await
        .unwrap();
    assert_eq!(answered.items[1].text, "canned:learning_feedback");

    let page = controller_timeline_items(&file, session.id).await;
    assert!(page
        .iter()
        .any(|item| item.text == "canned:learning_feedback"));
    let _ = std::fs::remove_file(file);
}

async fn controller_timeline_items(
    file: &std::path::Path,
    session_id: Uuid,
) -> Vec<talkcer_core::TimelineItem> {
    let pool = connect_and_migrate(file).await.unwrap();
    let timeline = TimelineService::new(SqliteTimelineRepository::new(pool));
    timeline
        .list(
            session_id,
            TimelineQuery {
                limit: 50,
                before: None,
            },
        )
        .await
        .unwrap()
        .items
}

#[test]
fn canned_thinker_reports_a_stable_provider_name() {
    assert_eq!(CannedThinker.provider(), "canned-thinker");
    // The registry kind is embedded so no consent gate applies to this fixture.
    assert_eq!(ProviderKind::Embedded.as_str(), "embedded");
}
