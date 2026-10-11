use talkcer_core::{
    connect_and_migrate, CreateSession, MockAction, MockInterviewService, MockTurn, SessionMode,
    SessionService, SessionStatus, SqliteSessionRepository, SqliteTimelineRepository, TimelineQuery,
    TimelineService,
};
use uuid::Uuid;

fn path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("mock-{name}-{}.sqlite", Uuid::now_v7()))
}

async fn service(file: &std::path::Path) -> MockInterviewService<SqliteSessionRepository, SqliteTimelineRepository> {
    let pool = connect_and_migrate(file).await.unwrap();
    MockInterviewService::new(
        SessionService::new(SqliteSessionRepository::new(pool.clone())),
        TimelineService::new(SqliteTimelineRepository::new(pool)),
    )
}

#[tokio::test]
async fn text_mock_interview_keeps_history() {
    let file = path("flow");
    let sessions = SessionService::new(SqliteSessionRepository::new(
        connect_and_migrate(&file).await.unwrap(),
    ));
    let session = sessions
        .create(CreateSession {
            title: "Backend mock".to_owned(),
            mode: SessionMode::MockInterview,
            context: serde_json::json!({ "role": "backend engineer" }),
            configuration: serde_json::json!({}),
        })
        .await
        .unwrap();
    drop(sessions);

    let controller = service(&file).await;
    let started = controller
        .turn(session.id, MockTurn { action: MockAction::Start, text: None })
        .await
        .unwrap();
    assert_eq!(started.status, SessionStatus::Active);
    assert_eq!(started.items[0].metadata["turn"], "question");
    assert!(started.items[0].text.contains("backend engineer"));

    let answered = controller
        .turn(
            session.id,
            MockTurn {
                action: MockAction::Answer,
                text: Some("I separated retries from the request path.".to_owned()),
            },
        )
        .await
        .unwrap();
    assert_eq!(answered.items.len(), 2);
    assert_eq!(answered.items[1].metadata["turn"], "follow_up");

    let completed = controller
        .turn(
            session.id,
            MockTurn {
                action: MockAction::Complete,
                text: Some("I would keep retries bounded.".to_owned()),
            },
        )
        .await
        .unwrap();
    assert_eq!(completed.status, SessionStatus::Completed);
    assert_eq!(completed.items[1].kind.as_str(), "feedback");
    assert_eq!(completed.items[2].metadata["turn"], "summary");

    let blank = controller
        .turn(session.id, MockTurn { action: MockAction::Answer, text: Some("  ".to_owned()) })
        .await
        .unwrap_err();
    assert!(blank.to_string().contains("must not be empty"));

    drop(controller);
    let reopened = service(&file).await;
    let pool = connect_and_migrate(&file).await.unwrap();
    let timeline = TimelineService::new(SqliteTimelineRepository::new(pool));
    let page = timeline
        .list(session.id, TimelineQuery { limit: 100, before: None })
        .await
        .unwrap();
    let turns: Vec<_> = page
        .items
        .iter()
        .map(|item| item.metadata["turn"].as_str().unwrap().to_owned())
        .collect();
    assert_eq!(turns, ["question", "answer", "follow_up", "answer", "feedback", "summary"]);
    let _ = reopened;
    let _ = std::fs::remove_file(file);
}
