//! Acceptance tests for transcript identity, ordering, and timeline persistence.

use std::path::PathBuf;

use serde_json::json;
use talkcer_core::{
    connect_and_migrate, stub_assistant_text, AssistantAction, AssistantActionRequest,
    CreateSession, ItemKind, SessionMode, SessionService, Source, Speaker, SqliteSessionRepository,
    SqliteTimelineRepository, TimelineQuery, TimelineService, UpsertTranscript,
};
use uuid::Uuid;

fn db_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("{name}-{}.sqlite", Uuid::now_v7()))
}

async fn open(name: &str) -> (
    SessionService<SqliteSessionRepository>,
    TimelineService<SqliteTimelineRepository>,
    sqlx::SqlitePool,
    PathBuf,
) {
    let path = db_path(name);
    let pool = connect_and_migrate(&path).await.unwrap();
    let sessions = SessionService::new(SqliteSessionRepository::new(pool.clone()));
    let timeline = TimelineService::new(SqliteTimelineRepository::new(pool.clone()));
    (sessions, timeline, pool, path)
}

fn partial(id: Option<Uuid>, text: &str, started_at_ms: i64, is_final: bool) -> UpsertTranscript {
    UpsertTranscript {
        utterance_id: id,
        speaker: Speaker::Remote,
        source: Source::SystemAudio,
        text: text.to_owned(),
        started_at_ms,
        ended_at_ms: None,
        is_final,
    }
}

#[tokio::test]
async fn three_partials_update_one_item_and_final_replaces_text() {
    let (sessions, timeline, pool, path) = open("partial").await;
    let session = sessions
        .create(CreateSession {
            title: "Live".into(),
            mode: SessionMode::LiveInterview,
            context: json!({}),
            configuration: json!({}),
        })
        .await
        .unwrap();

    let first = timeline
        .upsert_transcript(session.id, partial(None, "How", 12_000, false))
        .await
        .unwrap();
    assert!(first.created);
    let id = first.item.id;

    let second = timeline
        .upsert_transcript(session.id, partial(Some(id), "How would", 99_000, false))
        .await
        .unwrap();
    let third = timeline
        .upsert_transcript(
            session.id,
            UpsertTranscript {
                speaker: Speaker::LocalUser,
                ..partial(Some(id), "How would you", 12_000, false)
            },
        )
        .await
        .unwrap();
    assert!(!second.created && !third.created);
    assert_eq!(second.item.id, id);
    assert_eq!(third.item.started_at_ms, 12_000);
    assert_eq!(third.item.speaker, Speaker::Remote);

    let final_item = timeline
        .upsert_transcript(
            session.id,
            UpsertTranscript {
                ended_at_ms: Some(15_800),
                ..partial(Some(id), "How would you handle retries?", 12_000, true)
            },
        )
        .await
        .unwrap();
    assert!(!final_item.created);
    assert!(final_item.item.is_final);
    assert_eq!(final_item.item.text, "How would you handle retries?");
    assert_eq!(final_item.item.ended_at_ms, Some(15_800));
    assert_eq!(final_item.item.utterance_id, Some(id));

    let page = timeline
        .list(session.id, TimelineQuery { limit: 100, before: None })
        .await
        .unwrap();
    assert_eq!(page.items.len(), 1);

    let rejected = timeline
        .upsert_transcript(session.id, partial(Some(id), "late", 12_000, false))
        .await
        .unwrap_err();
    assert_eq!(rejected.to_string(), "utterance is final and cannot be updated");

    drop(timeline);
    drop(sessions);
    pool.close().await;
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn timeline_is_ordered_and_paginates_before_a_cursor() {
    let (sessions, timeline, pool, path) = open("order").await;
    let session = sessions
        .create(CreateSession {
            title: "Order".into(),
            mode: SessionMode::Learning,
            context: json!({}),
            configuration: json!({}),
        })
        .await
        .unwrap();
    for (text, started) in [("c", 300), ("a", 100), ("b", 200)] {
        timeline
            .upsert_transcript(session.id, partial(None, text, started, true))
            .await
            .unwrap();
    }

    let page = timeline
        .list(session.id, TimelineQuery { limit: 2, before: None })
        .await
        .unwrap();
    assert_eq!(
        page.items.iter().map(|item| item.text.as_str()).collect::<Vec<_>>(),
        ["b", "c"]
    );

    let older = timeline
        .list(
            session.id,
            TimelineQuery {
                limit: 10,
                before: Some(page.items[0].id),
            },
        )
        .await
        .unwrap();
    assert_eq!(older.items.len(), 1);
    assert_eq!(older.items[0].text, "a");

    drop(timeline);
    drop(sessions);
    pool.close().await;
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn timeline_persists_after_restart() {
    let path = db_path("timeline-restart");
    let session_id = {
        let pool = connect_and_migrate(&path).await.unwrap();
        let sessions = SessionService::new(SqliteSessionRepository::new(pool.clone()));
        let timeline = TimelineService::new(SqliteTimelineRepository::new(pool.clone()));
        let session = sessions
            .create(CreateSession {
                title: "Persist".into(),
                mode: SessionMode::MockInterview,
                context: json!({}),
                configuration: json!({}),
            })
            .await
            .unwrap();
        timeline
            .upsert_transcript(session.id, partial(None, "saved", 10, true))
            .await
            .unwrap();
        pool.close().await;
        session.id
    };

    let pool = connect_and_migrate(&path).await.unwrap();
    let timeline = TimelineService::new(SqliteTimelineRepository::new(pool));
    let page = timeline
        .list(session_id, TimelineQuery { limit: 10, before: None })
        .await
        .unwrap();
    assert_eq!(page.items[0].text, "saved");
    assert!(page.items[0].is_final);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn assistant_stub_writes_a_separate_ai_item_without_a_provider() {
    let (sessions, timeline, pool, path) = open("assistant").await;
    let session = sessions
        .create(CreateSession {
            title: "Assist".into(),
            mode: SessionMode::LiveInterview,
            context: json!({}),
            configuration: json!({}),
        })
        .await
        .unwrap();
    let transcript = timeline
        .upsert_transcript(session.id, partial(None, "retries", 1_000, true))
        .await
        .unwrap();
    let result = timeline
        .assistant_action(
            session.id,
            AssistantActionRequest {
                action: AssistantAction::Explain,
                target_id: transcript.item.id,
                language: Some("id".into()),
            },
        )
        .await
        .unwrap();

    assert_eq!(result.instruction.purpose.as_str(), "assistant_instruction");
    assert_eq!(result.message.kind, ItemKind::AiMessage);
    assert_eq!(
        result.message.text,
        stub_assistant_text(AssistantAction::Explain, transcript.item.id)
    );
    assert_eq!(result.message.metadata["stub"], true);
    assert_eq!(result.message.metadata["target"]["id"], transcript.item.id.to_string());
    assert!(result.message.metadata.get("utterance_id").is_none());

    let page = timeline
        .list(session.id, TimelineQuery { limit: 10, before: None })
        .await
        .unwrap();
    assert_eq!(page.items.len(), 3);
    assert_eq!(page.items[2].kind, ItemKind::AiMessage);

    drop(timeline);
    drop(sessions);
    pool.close().await;
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn blank_message_is_rejected() {
    let (sessions, timeline, pool, path) = open("blank").await;
    let session = sessions
        .create(CreateSession {
            title: "Blank".into(),
            mode: SessionMode::Learning,
            context: json!({}),
            configuration: json!({}),
        })
        .await
        .unwrap();
    let error = timeline
        .append_message(session.id, "   ".into(), json!({}))
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "message text must not be empty");
    drop(timeline);
    drop(sessions);
    pool.close().await;
    let _ = std::fs::remove_file(path);
}
