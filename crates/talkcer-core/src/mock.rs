//! Text-only mock interview controller.
//!
//! Question, follow-up, and feedback text come from the local controller.
//! Every turn is stored as its own timeline row so history survives restart.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::domain::{SessionMode, SessionStatus};
use crate::error::CoreError;
use crate::session::{SessionRepository, SessionService, UpdateSession};
use crate::timeline::{
    ItemKind, Purpose, Speaker, TimelineItem, TimelineRepository, TimelineService,
};

/// One step of a text mock interview.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MockAction {
    /// Move from ready to active and ask the first question.
    Start,
    /// Store the user's answer and ask a follow-up.
    Answer,
    /// Store the last answer, feedback, and the summary, then complete the session.
    Complete,
}

impl MockAction {
    /// Parse a wire action.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidMockAction`] for an unknown value.
    pub fn parse(value: &str) -> Result<Self, CoreError> {
        match value {
            "start" => Ok(Self::Start),
            "answer" => Ok(Self::Answer),
            "complete" => Ok(Self::Complete),
            other => Err(CoreError::InvalidMockAction(other.to_owned())),
        }
    }

    /// Wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Answer => "answer",
            Self::Complete => "complete",
        }
    }
}

/// Body of `POST /api/sessions/:id/mock`.
#[derive(Debug, Clone, Deserialize)]
pub struct MockTurn {
    /// Controller step.
    pub action: MockAction,
    /// Required for `answer` and `complete`.
    #[serde(default)]
    pub text: Option<String>,
}

/// Rows written by one controller step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MockTurnResult {
    /// Session status after the step.
    pub status: SessionStatus,
    /// Timeline rows created by the step, oldest first.
    pub items: Vec<TimelineItem>,
}

/// Coordinates session status and timeline writes for mock interviews.
#[derive(Clone)]
pub struct MockInterviewService<S, T> {
    sessions: SessionService<S>,
    timeline: TimelineService<T>,
}

impl<S, T> MockInterviewService<S, T>
where
    S: SessionRepository,
    T: TimelineRepository,
{
    /// Build a controller over the existing session and timeline services.
    #[must_use]
    pub fn new(sessions: SessionService<S>, timeline: TimelineService<T>) -> Self {
        Self { sessions, timeline }
    }

    /// Apply one text turn.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::NotFound`] when the session does not exist,
    /// [`CoreError::InvalidMockAction`] when the mode or status does not allow
    /// the step, and [`CoreError::EmptyText`] when an answer is blank.
    pub async fn turn(&self, session_id: Uuid, input: MockTurn) -> Result<MockTurnResult, CoreError> {
        let session = self.sessions.get(session_id).await?;
        if session.mode != SessionMode::MockInterview {
            return Err(CoreError::InvalidMockAction(
                "mock actions require a mock_interview session".to_owned(),
            ));
        }
        let role = session
            .context
            .get("role")
            .and_then(Value::as_str)
            .unwrap_or("software engineer");
        match input.action {
            MockAction::Start => self.start(session_id, session.status, role).await,
            MockAction::Answer => {
                let answer = required_text(input.text)?;
                self.answer(session_id, session.status, role, &answer).await
            }
            MockAction::Complete => {
                let answer = required_text(input.text)?;
                self.complete(session_id, session.status, &answer).await
            }
        }
    }

    async fn start(
        &self,
        session_id: Uuid,
        status: SessionStatus,
        role: &str,
    ) -> Result<MockTurnResult, CoreError> {
        if status != SessionStatus::Ready {
            return Err(CoreError::InvalidMockAction(format!(
                "start requires ready, found {}",
                status.as_str()
            )));
        }
        self.set_status(session_id, SessionStatus::Active).await?;
        let question = self
            .append(
                session_id,
                ItemKind::AiMessage,
                Speaker::Ai,
                Purpose::AssistantResponse,
                "question",
                &format!("Tell me about your experience as a {role}."),
            )
            .await?;
        Ok(MockTurnResult {
            status: SessionStatus::Active,
            items: vec![question],
        })
    }

    async fn answer(
        &self,
        session_id: Uuid,
        status: SessionStatus,
        role: &str,
        answer: &str,
    ) -> Result<MockTurnResult, CoreError> {
        if status != SessionStatus::Active {
            return Err(CoreError::InvalidMockAction(format!(
                "answer requires active, found {}",
                status.as_str()
            )));
        }
        let answer_item = self
            .append(
                session_id,
                ItemKind::UserMessage,
                Speaker::LocalUser,
                Purpose::Conversation,
                "answer",
                answer,
            )
            .await?;
        let follow_up = self
            .append(
                session_id,
                ItemKind::AiMessage,
                Speaker::Ai,
                Purpose::AssistantResponse,
                "follow_up",
                &format!("What tradeoff would you make in that {role} example?"),
            )
            .await?;
        Ok(MockTurnResult {
            status: SessionStatus::Active,
            items: vec![answer_item, follow_up],
        })
    }

    async fn complete(
        &self,
        session_id: Uuid,
        status: SessionStatus,
        answer: &str,
    ) -> Result<MockTurnResult, CoreError> {
        if status != SessionStatus::Active {
            return Err(CoreError::InvalidMockAction(format!(
                "complete requires active, found {}",
                status.as_str()
            )));
        }
        let answer_item = self
            .append(
                session_id,
                ItemKind::UserMessage,
                Speaker::LocalUser,
                Purpose::Conversation,
                "answer",
                answer,
            )
            .await?;
        let feedback = self
            .append(
                session_id,
                ItemKind::Feedback,
                Speaker::Ai,
                Purpose::AssistantResponse,
                "feedback",
                "Your answers stay on the requested topic and include a concrete example.",
            )
            .await?;
        let summary = self
            .append(
                session_id,
                ItemKind::AiMessage,
                Speaker::Ai,
                Purpose::AssistantResponse,
                "summary",
                "Summary: the interview covered experience and one tradeoff.",
            )
            .await?;
        self.set_status(session_id, SessionStatus::Completed).await?;
        Ok(MockTurnResult {
            status: SessionStatus::Completed,
            items: vec![answer_item, feedback, summary],
        })
    }

    async fn set_status(&self, session_id: Uuid, status: SessionStatus) -> Result<(), CoreError> {
        self.sessions
            .update(
                session_id,
                UpdateSession {
                    status: Some(status),
                    ..Default::default()
                },
            )
            .await?;
        Ok(())
    }

    async fn append(
        &self,
        session_id: Uuid,
        kind: ItemKind,
        speaker: Speaker,
        purpose: Purpose,
        turn: &str,
        text: &str,
    ) -> Result<TimelineItem, CoreError> {
        self.timeline
            .append_item(
                session_id,
                kind,
                speaker,
                purpose,
                text.to_owned(),
                json!({ "turn": turn }),
            )
            .await
    }
}

fn required_text(value: Option<String>) -> Result<String, CoreError> {
    let text = value.unwrap_or_default().trim().to_owned();
    if text.is_empty() {
        Err(CoreError::EmptyText)
    } else {
        Ok(text)
    }
}
