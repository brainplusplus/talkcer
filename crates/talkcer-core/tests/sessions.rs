//! Acceptance tests for Task 2 session CRUD.

use std::path::PathBuf;

use chrono::Utc;
use serde_json::json;
use talkcer_core::{
    connect_and_migrate, CreateSession, SessionFilter, SessionMode, SessionService, SessionStatus,
    SqliteSessionRepository, UpdateSession,
};
use uuid::Uuid;

fn db_path(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("{name}-{}.sqlite", Uuid::now_v7()))
}

async fn service(name: &str) -> (SessionService<SqliteSessionRepository>, PathBuf) {
    let path = db_path(name);
    let pool = connect_and_migrate(&path).await.unwrap();
    (SessionService::new(SqliteSessionRepository::new(pool)), path)
}

fn live(title: &str) -> CreateSession {
    CreateSession {
        title: title.to_owned(),
        mode: SessionMode::LiveInterview,
        context: json!({"role": "Backend Engineer", "company": "Acme", "language": "en"}),
        configuration: json!({"stt": "default", "thinker": "default", "tts": "disabled"}),
    }
}

#[tokio::test]
async fn invalid_mode_is_rejected() {
    let error = SessionMode::parse("interview").unwrap_err();
    assert_eq!(error.to_string(), "invalid session mode: interview");
    assert!(SessionMode::parse("live_interview").is_ok());
    assert!(SessionMode::parse("mock_interview").is_ok());
    assert!(SessionMode::parse("learning").is_ok());
}

#[tokio::test]
async fn create_list_get_update_delete_round_trip() {
    let (service, path) = service("crud").await;

    let created = service.create(live("Backend Engineer — Acme")).await.unwrap();
    assert_eq!(created.status, SessionStatus::Ready);
    assert_eq!(created.context["company"], "Acme");

    let loaded = service.get(created.id).await.unwrap();
    assert_eq!(loaded, created);

    let listed = service
        .list(&SessionFilter {
            mode: Some(SessionMode::LiveInterview),
            status: Some(SessionStatus::Ready),
        })
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);

    let updated = service
        .update(
            created.id,
            UpdateSession {
                title: Some("Updated title".into()),
                status: Some(SessionStatus::Active),
                context: None,
                configuration: None,
            },
        )
        .await
        .unwrap();
    assert_eq!(updated.title, "Updated title");
    assert_eq!(updated.status, SessionStatus::Active);
    assert!(updated.updated_at >= created.updated_at);

    service.delete(created.id).await.unwrap();
    assert!(service.get(created.id).await.is_err());
    drop(service);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn session_persists_after_connection_restart() {
    let path = db_path("restart");
    let id = {
        let pool = connect_and_migrate(&path).await.unwrap();
        let service = SessionService::new(SqliteSessionRepository::new(pool.clone()));
        let created = service.create(live("Persist me")).await.unwrap();
        pool.close().await;
        created.id
    };

    let pool = connect_and_migrate(&path).await.unwrap();
    let service = SessionService::new(SqliteSessionRepository::new(pool));
    let loaded = service.get(id).await.unwrap();
    assert_eq!(loaded.title, "Persist me");
    assert_eq!(loaded.mode, SessionMode::LiveInterview);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn sessions_are_grouped_by_mode() {
    let (service, path) = service("grouped").await;
    service.create(live("Live one")).await.unwrap();
    service
        .create(CreateSession {
            title: "Mock one".into(),
            mode: SessionMode::MockInterview,
            context: json!({}),
            configuration: json!({}),
        })
        .await
        .unwrap();
    service
        .create(CreateSession {
            title: "Learn one".into(),
            mode: SessionMode::Learning,
            context: json!({}),
            configuration: json!({}),
        })
        .await
        .unwrap();
    service.create(live("Live two")).await.unwrap();

    let grouped = service.grouped_by_mode().await.unwrap();
    assert_eq!(grouped.live_interview.len(), 2);
    assert_eq!(grouped.mock_interview.len(), 1);
    assert_eq!(grouped.learning.len(), 1);
    assert!(grouped
        .live_interview
        .iter()
        .all(|session| session.mode == SessionMode::LiveInterview));
    assert_eq!(grouped.mock_interview[0].title, "Mock one");
    assert_eq!(grouped.learning[0].title, "Learn one");
    drop(service);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn filter_excludes_other_modes_and_statuses() {
    let (service, path) = service("filter").await;
    let live = service.create(live("Live")).await.unwrap();
    service
        .update(
            live.id,
            UpdateSession {
                status: Some(SessionStatus::Active),
                ..UpdateSession::default()
            },
        )
        .await
        .unwrap();
    service
        .create(CreateSession {
            title: "Mock".into(),
            mode: SessionMode::MockInterview,
            context: json!({}),
            configuration: json!({}),
        })
        .await
        .unwrap();

    let only_active_live = service
        .list(&SessionFilter {
            mode: Some(SessionMode::LiveInterview),
            status: Some(SessionStatus::Active),
        })
        .await
        .unwrap();
    assert_eq!(only_active_live.len(), 1);
    assert_eq!(only_active_live[0].title, "Live");

    let ready_live = service
        .list(&SessionFilter {
            mode: Some(SessionMode::LiveInterview),
            status: Some(SessionStatus::Ready),
        })
        .await
        .unwrap();
    assert!(ready_live.is_empty());
    drop(service);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn empty_title_is_rejected() {
    let (service, path) = service("title").await;
    let error = service
        .create(CreateSession {
            title: "   ".into(),
            mode: SessionMode::Learning,
            context: json!(null),
            configuration: json!({}),
        })
        .await
        .unwrap_err();
    assert_eq!(error.to_string(), "session title must not be empty");
    drop(service);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn illegal_status_transition_is_rejected() {
    let (service, path) = service("transition").await;
    let created = service.create(live("Transition")).await.unwrap();

    let skipped = service
        .update(
            created.id,
            UpdateSession {
                status: Some(SessionStatus::Completed),
                ..UpdateSession::default()
            },
        )
        .await
        .unwrap_err();
    assert_eq!(
        skipped.to_string(),
        "invalid session transition: ready -> completed"
    );

    service
        .update(
            created.id,
            UpdateSession {
                status: Some(SessionStatus::Active),
                ..UpdateSession::default()
            },
        )
        .await
        .unwrap();
    service
        .update(
            created.id,
            UpdateSession {
                status: Some(SessionStatus::Paused),
                ..UpdateSession::default()
            },
        )
        .await
        .unwrap();
    let resumed = service
        .update(
            created.id,
            UpdateSession {
                status: Some(SessionStatus::Active),
                ..UpdateSession::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(resumed.status, SessionStatus::Active);

    let archived = service
        .update(
            created.id,
            UpdateSession {
                status: Some(SessionStatus::Archived),
                ..UpdateSession::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(archived.status, SessionStatus::Archived);
    drop(service);
    let _ = std::fs::remove_file(path);
}

#[tokio::test]
async fn unknown_status_string_is_rejected() {
    let error = SessionStatus::parse("running").unwrap_err();
    assert_eq!(error.to_string(), "invalid session status: running");
}

#[test]
fn migration_sql_matches_schema_tables() {
    let sql = talkcer_core::embedded_migrations().files()[0].1;
    for table in [
        "sessions",
        "timeline_items",
        "ai_configurations",
        "attachments",
        "overlay_preferences",
    ] {
        assert!(sql.contains(&format!("CREATE TABLE {table}")));
    }
}

#[tokio::test]
async fn timestamps_round_trip_as_utc() {
    let (service, path) = service("time").await;
    let before = Utc::now();
    let created = service.create(live("Timed")).await.unwrap();
    assert!(created.created_at >= before - chrono::Duration::seconds(2));
    let loaded = service.get(created.id).await.unwrap();
    assert_eq!(loaded.created_at, created.created_at);
    drop(service);
    let _ = std::fs::remove_file(path);
}
