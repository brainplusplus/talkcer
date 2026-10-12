//! SQLite schema migrations embedded from `migrations/`.
//!
//! Applied migrations are recorded and never rewritten. Later tasks add a new file.

use std::path::Path;

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::error::CoreError;

/// Ordered SQL files applied on startup.
pub struct MigrationSource {
    files: &'static [(&'static str, &'static str)],
}

impl MigrationSource {
    /// Built-in schema, currently the initial tables from the database spec.
    #[must_use]
    pub const fn embedded() -> Self {
        Self {
            files: EMBEDDED,
        }
    }

    /// Version and SQL pairs in apply order.
    #[must_use]
    pub const fn files(&self) -> &'static [(&'static str, &'static str)] {
        self.files
    }
}

const EMBEDDED: &[(&str, &str)] = &[
    (
        "001_init.sql",
        include_str!("../../../migrations/001_init.sql"),
    ),
    (
        "002_capture_remote.sql",
        include_str!("../../../migrations/002_capture_remote.sql"),
    ),
    (
        "003_overlay_session.sql",
        include_str!("../../../migrations/003_overlay_session.sql"),
    ),
    (
        "004_privacy.sql",
        include_str!("../../../migrations/004_privacy.sql"),
    ),
];

/// The migrations shipped with this crate.
#[must_use]
pub fn embedded_migrations() -> MigrationSource {
    MigrationSource::embedded()
}

/// Open a SQLite file, create it when missing, and apply pending migrations.
///
/// # Errors
///
/// Returns [`CoreError::Database`] when the file cannot be opened and
/// [`CoreError::Migration`] when a statement fails.
pub async fn connect_and_migrate(database_path: &Path) -> Result<SqlitePool, CoreError> {
    if let Some(parent) = database_path.parent() {
        if !parent.as_os_str().is_empty() {
            std::fs::create_dir_all(parent).map_err(|error| {
                CoreError::Migration(format!("create database directory: {error}"))
            })?;
        }
    }

    let options = SqliteConnectOptions::new()
        .filename(database_path)
        .create_if_missing(true)
        .foreign_keys(true);
    let pool = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options)
        .await?;
    apply(&pool).await?;
    Ok(pool)
}

/// Apply every embedded migration that is not yet recorded.
///
/// # Errors
///
/// Returns [`CoreError::Migration`] when SQL execution or bookkeeping fails.
pub async fn apply(pool: &SqlitePool) -> Result<(), CoreError> {
    sqlx::query(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version TEXT PRIMARY KEY,
            applied_at TEXT NOT NULL
        )",
    )
    .execute(pool)
    .await
    .map_err(|error| CoreError::Migration(error.to_string()))?;

    for (version, sql) in embedded_migrations().files() {
        let already: Option<(String,)> =
            sqlx::query_as("SELECT version FROM schema_migrations WHERE version = ?")
                .bind(*version)
                .fetch_optional(pool)
                .await
                .map_err(|error| CoreError::Migration(error.to_string()))?;
        if already.is_some() {
            continue;
        }

        let mut tx = pool
            .begin()
            .await
            .map_err(|error| CoreError::Migration(error.to_string()))?;
        for statement in split_sql(sql) {
            sqlx::query(&statement)
                .execute(&mut *tx)
                .await
                .map_err(|error| CoreError::Migration(format!("{version}: {error}")))?;
        }
        sqlx::query("INSERT INTO schema_migrations (version, applied_at) VALUES (?, ?)")
            .bind(*version)
            .bind(chrono::Utc::now().to_rfc3339())
            .execute(&mut *tx)
            .await
            .map_err(|error| CoreError::Migration(error.to_string()))?;
        tx.commit()
            .await
            .map_err(|error| CoreError::Migration(error.to_string()))?;
    }

    Ok(())
}

fn split_sql(sql: &str) -> Vec<String> {
    sql.split(';')
        .map(str::trim)
        .filter(|statement| !statement.is_empty())
        .map(ToOwned::to_owned)
        .collect()
}
