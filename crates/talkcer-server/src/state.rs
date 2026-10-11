//! Shared HTTP state.

use std::path::Path;

use talkcer_core::{
    AiConfigurationService, MockInterviewService, SessionService, SqliteAiConfigurationRepository,
    SqliteSessionRepository, SqliteTimelineRepository, TimelineService,
};
use tokio::sync::broadcast;

/// State available to every request handler.
#[derive(Clone)]
pub struct AppState {
    /// Session application service.
    pub sessions: SessionService<SqliteSessionRepository>,
    /// Timeline application service.
    pub timeline: TimelineService<SqliteTimelineRepository>,
    /// Saved STT, thinker, and TTS configuration.
    pub ai: AiConfigurationService<SqliteAiConfigurationRepository>,
    /// Text mock interview controller.
    pub mock: MockInterviewService<SqliteSessionRepository, SqliteTimelineRepository>,
    /// Broadcast channel for WebSocket subscribers. Payloads are JSON events.
    pub events: broadcast::Sender<String>,
}

impl AppState {
    /// Open SQLite and run pending migrations.
    ///
    /// # Errors
    ///
    /// Returns the underlying core error when the file cannot be opened.
    pub async fn connect(database_path: &Path) -> Result<Self, talkcer_core::CoreError> {
        let pool = talkcer_core::connect_and_migrate(database_path).await?;
        let (events, _) = broadcast::channel(256);
        let sessions = SessionService::new(SqliteSessionRepository::new(pool.clone()));
        let timeline = TimelineService::new(SqliteTimelineRepository::new(pool.clone()));
        Ok(Self {
            mock: MockInterviewService::new(sessions.clone(), timeline.clone()),
            sessions,
            timeline,
            ai: AiConfigurationService::new(SqliteAiConfigurationRepository::new(pool)),
            events,
        })
    }

    /// Publish one event. A lack of subscribers is not an error.
    pub fn publish(&self, event: &serde_json::Value) {
        let _ = self.events.send(event.to_string());
    }
}
