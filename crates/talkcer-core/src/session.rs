//! Session persistence and the CRUD service used by the HTTP API.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::domain::{Session, SessionMode, SessionStatus};
use crate::error::CoreError;

/// Fields required to create a session.
#[derive(Debug, Clone, Deserialize)]
pub struct CreateSession {
    /// User-facing title.
    pub title: String,
    /// Conversation category.
    pub mode: SessionMode,
    /// Optional mode context. Missing values are stored as `{}`.
    #[serde(default)]
    pub context: Value,
    /// Optional capability selection. Missing values are stored as `{}`.
    #[serde(default)]
    pub configuration: Value,
}

/// Fields that may change on an existing session.
///
/// Absent fields are left unchanged. `context` and `configuration` replace
/// the stored JSON object when present.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct UpdateSession {
    /// Replacement title.
    pub title: Option<String>,
    /// Replacement lifecycle status.
    pub status: Option<SessionStatus>,
    /// Replacement context object.
    pub context: Option<Value>,
    /// Replacement configuration object.
    pub configuration: Option<Value>,
}

/// Query filters for the session list.
#[derive(Debug, Clone, Default)]
pub struct SessionFilter {
    /// Restrict results to one mode.
    pub mode: Option<SessionMode>,
    /// Restrict results to one status.
    pub status: Option<SessionStatus>,
}

/// Sessions grouped by the three product modes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SessionsByMode {
    /// Live interview sessions, newest first.
    pub live_interview: Vec<Session>,
    /// Mock interview sessions, newest first.
    pub mock_interview: Vec<Session>,
    /// Learning sessions, newest first.
    pub learning: Vec<Session>,
}

/// Persistence port for sessions.
#[async_trait::async_trait]
pub trait SessionRepository: Send + Sync {
    /// Insert a fully built session.
    async fn insert(&self, session: &Session) -> Result<(), CoreError>;

    /// Load one session.
    async fn get(&self, id: Uuid) -> Result<Option<Session>, CoreError>;

    /// List sessions, optionally filtered, newest update first.
    async fn list(&self, filter: &SessionFilter) -> Result<Vec<Session>, CoreError>;

    /// Replace mutable columns of an existing session.
    async fn update(&self, session: &Session) -> Result<(), CoreError>;

    /// Remove a session. Timeline rows cascade when that table is used.
    async fn delete(&self, id: Uuid) -> Result<bool, CoreError>;
}

/// SQLite implementation of [`SessionRepository`].
#[derive(Clone)]
pub struct SqliteSessionRepository {
    pool: SqlitePool,
}

impl SqliteSessionRepository {
    /// Wrap a migrated SQLite pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl SessionRepository for SqliteSessionRepository {
    async fn insert(&self, session: &Session) -> Result<(), CoreError> {
        sqlx::query(
            "INSERT INTO sessions
             (id, title, mode, status, context, configuration, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(session.id.to_string())
        .bind(&session.title)
        .bind(session.mode.as_str())
        .bind(session.status.as_str())
        .bind(session.context.to_string())
        .bind(session.configuration.to_string())
        .bind(session.created_at.to_rfc3339())
        .bind(session.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<Session>, CoreError> {
        let row = sqlx::query(
            "SELECT id, title, mode, status, context, configuration, created_at, updated_at
             FROM sessions WHERE id = ?",
        )
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(session_from_row).transpose()
    }

    async fn list(&self, filter: &SessionFilter) -> Result<Vec<Session>, CoreError> {
        let rows = sqlx::query(
            "SELECT id, title, mode, status, context, configuration, created_at, updated_at
             FROM sessions
             WHERE (? IS NULL OR mode = ?)
               AND (? IS NULL OR status = ?)
             ORDER BY updated_at DESC, id DESC",
        )
        .bind(filter.mode.map(SessionMode::as_str))
        .bind(filter.mode.map(SessionMode::as_str))
        .bind(filter.status.map(SessionStatus::as_str))
        .bind(filter.status.map(SessionStatus::as_str))
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter().map(session_from_row).collect()
    }

    async fn update(&self, session: &Session) -> Result<(), CoreError> {
        sqlx::query(
            "UPDATE sessions
             SET title = ?, status = ?, context = ?, configuration = ?, updated_at = ?
             WHERE id = ?",
        )
        .bind(&session.title)
        .bind(session.status.as_str())
        .bind(session.context.to_string())
        .bind(session.configuration.to_string())
        .bind(session.updated_at.to_rfc3339())
        .bind(session.id.to_string())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn delete(&self, id: Uuid) -> Result<bool, CoreError> {
        let result = sqlx::query("DELETE FROM sessions WHERE id = ?")
            .bind(id.to_string())
            .execute(&self.pool)
            .await?;
        Ok(result.rows_affected() > 0)
    }
}

fn session_from_row(row: sqlx::sqlite::SqliteRow) -> Result<Session, CoreError> {
    let id: String = row.try_get("id")?;
    let mode: String = row.try_get("mode")?;
    let status: String = row.try_get("status")?;
    let context: String = row.try_get("context")?;
    let configuration: String = row.try_get("configuration")?;
    let created_at: String = row.try_get("created_at")?;
    let updated_at: String = row.try_get("updated_at")?;
    Ok(Session {
        id: Uuid::parse_str(&id).map_err(|error| {
            CoreError::Database(sqlx::Error::Decode(Box::new(error)))
        })?,
        title: row.try_get("title")?,
        mode: SessionMode::parse(&mode)?,
        status: SessionStatus::parse(&status)?,
        context: serde_json::from_str(&context).unwrap_or_else(|_| json!({})),
        configuration: serde_json::from_str(&configuration).unwrap_or_else(|_| json!({})),
        created_at: parse_time(&created_at)?,
        updated_at: parse_time(&updated_at)?,
    })
}

fn parse_time(value: &str) -> Result<DateTime<Utc>, CoreError> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|error| CoreError::Database(sqlx::Error::Decode(Box::new(error))))
}

/// Application service for session CRUD.
#[derive(Clone)]
pub struct SessionService<R> {
    repository: Arc<R>,
}

impl<R> SessionService<R>
where
    R: SessionRepository,
{
    /// Build a service over a repository.
    #[must_use]
    pub fn new(repository: R) -> Self {
        Self {
            repository: Arc::new(repository),
        }
    }

    /// Create a session in `ready` status.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::EmptyTitle`] when the title is blank, and a
    /// database error when the insert fails.
    pub async fn create(&self, input: CreateSession) -> Result<Session, CoreError> {
        let title = input.title.trim().to_owned();
        if title.is_empty() {
            return Err(CoreError::EmptyTitle);
        }
        let now = Utc::now();
        let session = Session {
            id: Uuid::now_v7(),
            title,
            mode: input.mode,
            status: SessionStatus::Ready,
            context: object_or_empty(input.context),
            configuration: object_or_empty(input.configuration),
            created_at: now,
            updated_at: now,
        };
        self.repository.insert(&session).await?;
        Ok(session)
    }

    /// Return one session or [`CoreError::NotFound`].
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::NotFound`] when the id is unknown.
    pub async fn get(&self, id: Uuid) -> Result<Session, CoreError> {
        self.repository
            .get(id)
            .await?
            .ok_or(CoreError::NotFound)
    }

    /// List sessions matching the optional mode and status filters.
    ///
    /// # Errors
    ///
    /// Returns a database error when the query fails.
    pub async fn list(&self, filter: &SessionFilter) -> Result<Vec<Session>, CoreError> {
        self.repository.list(filter).await
    }

    /// Group every session by mode, newest update first inside each group.
    ///
    /// # Errors
    ///
    /// Returns a database error when the query fails.
    pub async fn grouped_by_mode(&self) -> Result<SessionsByMode, CoreError> {
        let sessions = self.repository.list(&SessionFilter::default()).await?;
        let mut grouped = SessionsByMode {
            live_interview: Vec::new(),
            mock_interview: Vec::new(),
            learning: Vec::new(),
        };
        for session in sessions {
            match session.mode {
                SessionMode::LiveInterview => grouped.live_interview.push(session),
                SessionMode::MockInterview => grouped.mock_interview.push(session),
                SessionMode::Learning => grouped.learning.push(session),
            }
        }
        Ok(grouped)
    }

    /// Apply a partial update.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::NotFound`] when the session does not exist,
    /// [`CoreError::EmptyTitle`] when the new title is blank, and
    /// [`CoreError::InvalidTransition`] when `status` skips the state machine.
    pub async fn update(&self, id: Uuid, input: UpdateSession) -> Result<Session, CoreError> {
        let mut session = self.get(id).await?;
        if let Some(title) = input.title {
            let title = title.trim().to_owned();
            if title.is_empty() {
                return Err(CoreError::EmptyTitle);
            }
            session.title = title;
        }
        if let Some(status) = input.status {
            ensure_transition(session.status, status)?;
            session.status = status;
        }
        if let Some(context) = input.context {
            session.context = object_or_empty(context);
        }
        if let Some(configuration) = input.configuration {
            session.configuration = object_or_empty(configuration);
        }
        session.updated_at = Utc::now();
        self.repository.update(&session).await?;
        Ok(session)
    }

    /// Delete a session.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::NotFound`] when the id is unknown.
    pub async fn delete(&self, id: Uuid) -> Result<(), CoreError> {
        if self.repository.delete(id).await? {
            Ok(())
        } else {
            Err(CoreError::NotFound)
        }
    }
}

fn ensure_transition(from: SessionStatus, to: SessionStatus) -> Result<(), CoreError> {
    if from == to {
        return Ok(());
    }
    let allowed = matches!(
        (from, to),
        (SessionStatus::Ready, SessionStatus::Active)
            | (SessionStatus::Active, SessionStatus::Paused)
            | (SessionStatus::Paused, SessionStatus::Active)
            | (SessionStatus::Active, SessionStatus::Completed)
            | (_, SessionStatus::Archived)
    );
    if allowed {
        Ok(())
    } else {
        Err(CoreError::InvalidTransition {
            from: from.as_str(),
            to: to.as_str(),
        })
    }
}

fn object_or_empty(value: Value) -> Value {
    if value.is_object() {
        value
    } else if value.is_null() {
        json!({})
    } else {
        value
    }
}
