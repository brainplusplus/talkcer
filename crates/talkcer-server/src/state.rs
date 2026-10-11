//! Shared HTTP state.

use std::path::Path;

use talkcer_core::{SessionService, SqliteSessionRepository, SqliteTimelineRepository, TimelineService};
use tokio::sync::broadcast;

/// State available to every request handler.
#[derive(Clone)]
pub struct AppState {
    /// Session application service.
    pub sessions: SessionService<SqliteSessionRepository>,
    /// Timeline application service.
    pub timeline: TimelineService<SqliteTimelineRepository>,
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
        Ok(Self {
            sessions: SessionService::new(SqliteSessionRepository::new(pool.clone())),
            timeline: TimelineService::new(SqliteTimelineRepository::new(pool)),
            events,
        })
    }

    /// Publish one event. A lack of subscribers is not an error.
    pub fn publish(&self, event: &serde_json::Value) {
        let _ = self.events.send(event.to_string());
    }
}
