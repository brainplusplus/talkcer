use talkcer_core::{
    connect_and_migrate, CreateSession, LearningAction, LearningService, LearningTurn, SessionMode,
    SessionService, SessionStatus, SqliteSessionRepository, SqliteTimelineRepository,
    TimelineQuery, TimelineService,
};
use uuid::Uuid;

fn path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("learning-{name}-{}.sqlite", Uuid::now_v7()))
}

async fn service(
    file: &std::path::Path,
) -> LearningService<SqliteSessionRepository, SqliteTimelineRepository> {
    let pool = connect_and_migrate(file).await.unwrap();
    LearningService::new(
        SessionService::new(SqliteSessionRepository::new(pool.clone())),
        TimelineService::new(SqliteTimelineRepository::new(pool)),
    )
}

#[tokio::test]
async fn learning_session_can_continue_after_reopen() {
    let file = path("continue");
    let sessions = SessionService::new(SqliteSessionRepository::new(
        connect_and_migrate(&file).await.unwrap(),
    ));
    let session = sessions
        .create(CreateSession {
            title: "PostgreSQL".to_owned(),
            mode: SessionMode::Learning,
            context: serde_json::json!({ "topic": "PostgreSQL query optimization" }),
            configuration: serde_json::json!({}),
        })
        .await
        .unwrap();
    drop(sessions);

    let controller = service(&file).await;
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
    assert_eq!(started.status, SessionStatus::Active);
    assert!(started.items[0]
        .text
        .contains("PostgreSQL query optimization"));
    assert_eq!(started.items[1].metadata["turn"], "practice");

    controller
        .turn(
            session.id,
            LearningTurn {
                action: LearningAction::Discuss,
                text: Some("When should I add an index?".to_owned()),
            },
        )
        .await
        .unwrap();
    controller
        .turn(
            session.id,
            LearningTurn {
                action: LearningAction::Answer,
                text: Some("Add an index when the filter is selective.".to_owned()),
            },
        )
        .await
        .unwrap();
    drop(controller);

    let reopened = service(&file).await;
    let continued = reopened
        .turn(
            session.id,
            LearningTurn {
                action: LearningAction::Discuss,
                text: Some("How do I read EXPLAIN?".to_owned()),
            },
        )
        .await
        .unwrap();
    assert_eq!(continued.status, SessionStatus::Active);

    let timeline = TimelineService::new(SqliteTimelineRepository::new(
        connect_and_migrate(&file).await.unwrap(),
    ));
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
    let turns: Vec<_> = page
        .items
        .iter()
        .map(|item| item.metadata["turn"].as_str().unwrap())
        .collect();
    assert_eq!(
        turns,
        [
            "discussion",
            "practice",
            "question",
            "discussion",
            "practice",
            "practice_answer",
            "feedback",
            "question",
            "discussion",
            "practice"
        ]
    );
    let _ = std::fs::remove_file(file);
}
