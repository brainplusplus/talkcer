//! Shared HTTP state.

use std::path::Path;

use talkcer_core::{SessionService, SqliteSessionRepository};

/// State available to every request handler.
#[derive(Clone)]
pub struct AppState {
    /// Session application service.
    pub sessions: SessionService<SqliteSessionRepository>,
}

impl AppState {
    /// Open SQLite and run pending migrations.
    ///
    /// # Errors
    ///
    /// Returns the underlying core error when the file cannot be opened.
    pub async fn connect(database_path: &Path) -> Result<Self, talkcer_core::CoreError> {
        let pool = talkcer_core::connect_and_migrate(database_path).await?;
        Ok(Self {
            sessions: SessionService::new(SqliteSessionRepository::new(pool)),
        })
    }
}
