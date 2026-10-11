//! Shared HTTP state.

use std::path::Path;

use talkcer_core::{
    AiConfigurationService, CaptureService, LearningService, MockInterviewService, RemoteAccessService,
    SessionService, SqliteAiConfigurationRepository, SqliteSessionRepository, SqliteTimelineRepository,
    TimelineService, VoiceService,
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
    /// Text learning controller.
    pub learning: LearningService<SqliteSessionRepository, SqliteTimelineRepository>,
    /// Mock browser voice adapter.
    pub voice: VoiceService<SqliteSessionRepository, SqliteTimelineRepository>,
    /// Windows fixture capture controller.
    pub capture: CaptureService<SqliteSessionRepository, SqliteTimelineRepository>,
    /// Pairing gate for a second computer.
    pub remote: RemoteAccessService,
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
            learning: LearningService::new(sessions.clone(), timeline.clone()),
            voice: VoiceService::new(sessions.clone(), timeline.clone()),
            capture: CaptureService::new(sessions.clone(), timeline.clone(), pool.clone()),
            remote: RemoteAccessService::new(pool.clone()),
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
