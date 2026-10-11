//! Session domain types.
//!
//! Wire values match the database checks and the HTTP API:
//! `live_interview`, `mock_interview`, `learning`,
//! `ready`, `active`, `paused`, `completed`, `archived`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::CoreError;

/// Conversation category for a session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionMode {
    /// Live conversation with a human, assisted by AI.
    LiveInterview,
    /// Practice interview driven by the AI interviewer.
    MockInterview,
    /// Tutor conversation about a topic.
    Learning,
}

impl SessionMode {
    /// Parse a stored or request-body mode.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidMode`] when `value` is not a known mode.
    pub fn parse(value: &str) -> Result<Self, CoreError> {
        match value {
            "live_interview" => Ok(Self::LiveInterview),
            "mock_interview" => Ok(Self::MockInterview),
            "learning" => Ok(Self::Learning),
            other => Err(CoreError::InvalidMode(other.to_owned())),
        }
    }

    /// Stable snake_case value stored in SQLite and returned by the API.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LiveInterview => "live_interview",
            Self::MockInterview => "mock_interview",
            Self::Learning => "learning",
        }
    }
}

/// Lifecycle state of a session.
///
/// Legal transitions are `Ready -> Active -> Paused -> Active`,
/// `Active -> Completed`, and any state to `Archived`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    /// Created and not yet started.
    Ready,
    /// Conversation is in progress.
    Active,
    /// Conversation is paused and can resume.
    Paused,
    /// Conversation finished.
    Completed,
    /// Hidden from the active lists.
    Archived,
}

impl SessionStatus {
    /// Parse a stored or request-body status.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidStatus`] when `value` is not a known status.
    pub fn parse(value: &str) -> Result<Self, CoreError> {
        match value {
            "ready" => Ok(Self::Ready),
            "active" => Ok(Self::Active),
            "paused" => Ok(Self::Paused),
            "completed" => Ok(Self::Completed),
            "archived" => Ok(Self::Archived),
            other => Err(CoreError::InvalidStatus(other.to_owned())),
        }
    }

    /// Stable snake_case value stored in SQLite and returned by the API.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Ready => "ready",
            Self::Active => "active",
            Self::Paused => "paused",
            Self::Completed => "completed",
            Self::Archived => "archived",
        }
    }
}

/// A persisted conversation session.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Session {
    /// Sortable UUIDv7 identifier.
    pub id: Uuid,
    /// User-facing title.
    pub title: String,
    /// Conversation category.
    pub mode: SessionMode,
    /// Lifecycle state.
    pub status: SessionStatus,
    /// Mode-specific context such as role, company, or topic.
    pub context: serde_json::Value,
    /// Capability selection such as STT, thinker, and TTS.
    pub configuration: serde_json::Value,
    /// Creation time in UTC.
    pub created_at: DateTime<Utc>,
    /// Last mutation time in UTC.
    pub updated_at: DateTime<Utc>,
}
