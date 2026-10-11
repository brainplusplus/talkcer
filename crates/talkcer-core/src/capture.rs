//! Windows live-capture controller.
//!
//! The native WASAPI loopback is not opened here. Tests inject audio fixtures,
//! and the controller keeps the microphone and system-audio transcripts apart.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::domain::SessionMode;
use crate::error::CoreError;
use crate::session::{SessionRepository, SessionService};
use crate::timeline::{Source, Speaker, TimelineRepository, TimelineService, UpsertTranscript};

/// Persisted capture lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureState {
    /// Not capturing.
    Idle,
    /// Both channels are open.
    Capturing,
    /// Channels are open but fixtures are rejected.
    Paused,
    /// A device disconnect was recorded.
    Error,
}

impl CaptureState {
    fn parse(value: &str) -> Result<Self, CoreError> {
        match value {
            "idle" => Ok(Self::Idle),
            "capturing" => Ok(Self::Capturing),
            "paused" => Ok(Self::Paused),
            "error" => Ok(Self::Error),
            other => Err(CoreError::InvalidCapture(other.to_owned())),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Capturing => "capturing",
            Self::Paused => "paused",
            Self::Error => "error",
        }
    }
}

/// Capture control request.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureAction {
    /// Open both Windows channels.
    Start,
    /// Stop accepting fixtures without closing history.
    Pause,
    /// Accept fixtures again.
    Resume,
    /// Return to idle.
    Stop,
    /// Record a device disconnect.
    Disconnect,
    /// Leave the error state and return to idle.
    Recover,
}

/// Current capture row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaptureStatus {
    /// Live session id.
    pub session_id: Uuid,
    /// The only implemented platform.
    pub platform: &'static str,
    /// Lifecycle state.
    pub state: CaptureState,
    /// Disconnect detail, when the state is `error`.
    pub error: Option<String>,
}

/// Fixture injected into one channel.
#[derive(Debug, Clone, Deserialize)]
pub struct CaptureFixture {
    /// `microphone` for the local user or `system_audio` for the remote person.
    pub channel: String,
    /// Existing utterance for a later partial or final.
    #[serde(default)]
    pub utterance_id: Option<Uuid>,
    /// Fixture transcript.
    pub text: String,
    /// Ordering timestamp.
    pub started_at_ms: i64,
    /// Closing update.
    #[serde(default)]
    pub is_final: bool,
}

/// Windows-only capture controller backed by audio fixtures.
#[derive(Clone)]
pub struct CaptureService<S, T> {
    sessions: SessionService<S>,
    timeline: TimelineService<T>,
    pool: SqlitePool,
}

impl<S, T> CaptureService<S, T>
where
    S: SessionRepository,
    T: TimelineRepository,
{
    /// Build a controller over the shared pool.
    #[must_use]
    pub fn new(sessions: SessionService<S>, timeline: TimelineService<T>, pool: SqlitePool) -> Self {
        Self { sessions, timeline, pool }
    }

    /// Apply one lifecycle action.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidCapture`] for a non-live session or an illegal transition.
    pub async fn control(&self, session_id: Uuid, action: CaptureAction) -> Result<CaptureStatus, CoreError> {
        self.require_live(session_id).await?;
        let current = self.status(session_id).await?;
        let (state, error) = match (current.state, action) {
            (CaptureState::Idle | CaptureState::Error, CaptureAction::Start) => (CaptureState::Capturing, None),
            (CaptureState::Capturing, CaptureAction::Pause) => (CaptureState::Paused, None),
            (CaptureState::Paused, CaptureAction::Resume) => (CaptureState::Capturing, None),
            (_, CaptureAction::Stop) => (CaptureState::Idle, None),
            (CaptureState::Capturing | CaptureState::Paused, CaptureAction::Disconnect) => {
                (CaptureState::Error, Some("audio device disconnected".to_owned()))
            }
            (CaptureState::Error, CaptureAction::Recover) => (CaptureState::Idle, None),
            (state, _) => {
                return Err(CoreError::InvalidCapture(format!(
                    "cannot apply action while capture is {}",
                    state.as_str()
                )))
            }
        };
        self.store(session_id, state, error.clone()).await?;
        Ok(CaptureStatus { session_id, platform: "windows", state, error })
    }

    /// Return the stored state, or idle when capture has not started.
    ///
    /// # Errors
    ///
    /// Returns a database error when the row cannot be read.
    pub async fn status(&self, session_id: Uuid) -> Result<CaptureStatus, CoreError> {
        let row = sqlx::query("SELECT state, error FROM capture_sessions WHERE session_id = ?")
            .bind(session_id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        let Some(row) = row else {
            return Ok(CaptureStatus { session_id, platform: "windows", state: CaptureState::Idle, error: None });
        };
        Ok(CaptureStatus {
            session_id,
            platform: "windows",
            state: CaptureState::parse(&row.try_get::<String, _>("state")?)?,
            error: row.try_get("error")?,
        })
    }

    /// Turn one fixture into a separate local or remote transcript.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidCapture`] unless capture is currently `capturing`.
    pub async fn inject(
        &self,
        session_id: Uuid,
        fixture: CaptureFixture,
    ) -> Result<crate::timeline::TranscriptUpsert, CoreError> {
        let status = self.status(session_id).await?;
        if status.state != CaptureState::Capturing {
            return Err(CoreError::InvalidCapture(format!(
                "fixture requires capturing, found {}",
                status.state.as_str()
            )));
        }
        let (speaker, source) = match fixture.channel.as_str() {
            "microphone" => (Speaker::LocalUser, Source::Microphone),
            "system_audio" => (Speaker::Remote, Source::SystemAudio),
            other => return Err(CoreError::InvalidCapture(format!("unknown channel {other}"))),
        };
        self.timeline
            .upsert_transcript(
                session_id,
                UpsertTranscript {
                    utterance_id: fixture.utterance_id,
                    speaker,
                    source,
                    text: fixture.text.trim().to_owned(),
                    started_at_ms: fixture.started_at_ms,
                    ended_at_ms: fixture.is_final.then_some(fixture.started_at_ms.saturating_add(1)),
                    is_final: fixture.is_final,
                    metadata: Some(json!({ "audio_channel": fixture.channel, "platform": "windows" })),
                },
            )
            .await
    }

    async fn require_live(&self, session_id: Uuid) -> Result<(), CoreError> {
        let session = self.sessions.get(session_id).await?;
        if session.mode == SessionMode::LiveInterview {
            Ok(())
        } else {
            Err(CoreError::InvalidCapture("capture requires a live_interview session".to_owned()))
        }
    }

    async fn store(&self, session_id: Uuid, state: CaptureState, error: Option<String>) -> Result<(), CoreError> {
        sqlx::query(
            "INSERT INTO capture_sessions (session_id, platform, state, error, updated_at)
             VALUES (?, 'windows', ?, ?, ?)
             ON CONFLICT(session_id) DO UPDATE SET
                state = excluded.state,
                error = excluded.error,
                updated_at = excluded.updated_at",
        )
        .bind(session_id.to_string())
        .bind(state.as_str())
        .bind(error)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
