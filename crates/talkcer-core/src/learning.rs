//! Text learning controller.
//!
//! A session stays active so the learner can leave and continue later.
//! Discussion, practice, and feedback are separate timeline rows.

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::domain::{SessionMode, SessionStatus};
use crate::error::CoreError;
use crate::session::{SessionRepository, SessionService, UpdateSession};
use crate::thinker::ThinkerTask;
use crate::timeline::{
    ItemKind, Purpose, Speaker, TimelineItem, TimelineRepository, TimelineService,
};

/// One step of a text learning session.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LearningAction {
    /// Move from ready to active and introduce the topic.
    Start,
    /// Store a learner question, an answer, and a practice prompt.
    Discuss,
    /// Store a practice answer and feedback.
    Answer,
}

impl LearningAction {
    /// Parse a wire action.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidLearningAction`] for an unknown value.
    pub fn parse(value: &str) -> Result<Self, CoreError> {
        match value {
            "start" => Ok(Self::Start),
            "discuss" => Ok(Self::Discuss),
            "answer" => Ok(Self::Answer),
            other => Err(CoreError::InvalidLearningAction(other.to_owned())),
        }
    }

    /// Wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Start => "start",
            Self::Discuss => "discuss",
            Self::Answer => "answer",
        }
    }
}

/// Body of `POST /api/sessions/:id/learning`.
#[derive(Debug, Clone, Deserialize)]
pub struct LearningTurn {
    /// Controller step.
    pub action: LearningAction,
    /// Required for `discuss` and `answer`.
    #[serde(default)]
    pub text: Option<String>,
}

/// Rows written by one learning step.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LearningTurnResult {
    /// Session status after the step. Learning stays `active` until the user changes it.
    pub status: SessionStatus,
    /// Timeline rows created by the step, oldest first.
    pub items: Vec<TimelineItem>,
}

/// Coordinates learning turns without calling a model.
#[derive(Clone)]
pub struct LearningService<S, T> {
    sessions: SessionService<S>,
    timeline: TimelineService<T>,
}

impl<S, T> LearningService<S, T>
where
    S: SessionRepository,
    T: TimelineRepository,
{
    /// Build a controller over the existing services.
    #[must_use]
    pub fn new(sessions: SessionService<S>, timeline: TimelineService<T>) -> Self {
        Self { sessions, timeline }
    }

    /// Apply one text turn.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::NotFound`] when the session does not exist,
    /// [`CoreError::InvalidLearningAction`] when the mode or status does not
    /// allow the step, and [`CoreError::EmptyText`] when required text is blank.
    pub async fn turn(
        &self,
        session_id: Uuid,
        input: LearningTurn,
    ) -> Result<LearningTurnResult, CoreError> {
        let session = self.sessions.get(session_id).await?;
        if session.mode != SessionMode::Learning {
            return Err(CoreError::InvalidLearningAction(
                "learning actions require a learning session".to_owned(),
            ));
        }
        let topic = session
            .context
            .get("topic")
            .and_then(Value::as_str)
            .unwrap_or("the selected topic")
            .to_owned();
        match input.action {
            LearningAction::Start => self.start(&session, topic).await,
            LearningAction::Discuss => {
                self.require_active(session.status, "discuss")?;
                let text = required_text(input.text)?;
                self.discuss(&session, topic, &text).await
            }
            LearningAction::Answer => {
                self.require_active(session.status, "answer")?;
                let text = required_text(input.text)?;
                self.answer(&session, topic, &text).await
            }
        }
    }

    async fn start(
        &self,
        session: &crate::domain::Session,
        topic: String,
    ) -> Result<LearningTurnResult, CoreError> {
        if session.status != SessionStatus::Ready {
            return Err(CoreError::InvalidLearningAction(format!(
                "start requires ready, found {}",
                session.status.as_str()
            )));
        }
        let session_id = session.id;
        self.sessions
            .update(
                session_id,
                UpdateSession {
                    status: Some(SessionStatus::Active),
                    ..Default::default()
                },
            )
            .await?;
        let intro = self
            .think(
                session,
                ThinkerTask::LearningIntro {
                    topic: topic.clone(),
                },
            )
            .await?;
        let discussion = self
            .append(
                session_id,
                ItemKind::AiMessage,
                Speaker::Ai,
                Purpose::AssistantResponse,
                "discussion",
                &intro,
            )
            .await?;
        let practice_text = self
            .think(session, ThinkerTask::LearningStartPractice { topic })
            .await?;
        let practice = self
            .append(
                session_id,
                ItemKind::AiMessage,
                Speaker::Ai,
                Purpose::AssistantResponse,
                "practice",
                &practice_text,
            )
            .await?;
        Ok(LearningTurnResult {
            status: SessionStatus::Active,
            items: vec![discussion, practice],
        })
    }

    async fn discuss(
        &self,
        session: &crate::domain::Session,
        topic: String,
        text: &str,
    ) -> Result<LearningTurnResult, CoreError> {
        let session_id = session.id;
        let question = self
            .append(
                session_id,
                ItemKind::UserMessage,
                Speaker::LocalUser,
                Purpose::Conversation,
                "question",
                text,
            )
            .await?;
        let answer_text = self
            .think(
                session,
                ThinkerTask::LearningAnswer {
                    topic: topic.clone(),
                    question: text.to_owned(),
                },
            )
            .await?;
        let answer = self
            .append(
                session_id,
                ItemKind::AiMessage,
                Speaker::Ai,
                Purpose::AssistantResponse,
                "discussion",
                &answer_text,
            )
            .await?;
        let practice_text = self
            .think(session, ThinkerTask::LearningPractice { topic })
            .await?;
        let practice = self
            .append(
                session_id,
                ItemKind::AiMessage,
                Speaker::Ai,
                Purpose::AssistantResponse,
                "practice",
                &practice_text,
            )
            .await?;
        Ok(LearningTurnResult {
            status: SessionStatus::Active,
            items: vec![question, answer, practice],
        })
    }

    async fn answer(
        &self,
        session: &crate::domain::Session,
        topic: String,
        text: &str,
    ) -> Result<LearningTurnResult, CoreError> {
        let session_id = session.id;
        let answer = self
            .append(
                session_id,
                ItemKind::UserMessage,
                Speaker::LocalUser,
                Purpose::Conversation,
                "practice_answer",
                text,
            )
            .await?;
        let feedback_text = self
            .think(session, ThinkerTask::LearningFeedback { topic })
            .await?;
        let feedback = self
            .append(
                session_id,
                ItemKind::Feedback,
                Speaker::Ai,
                Purpose::AssistantResponse,
                "feedback",
                &feedback_text,
            )
            .await?;
        Ok(LearningTurnResult {
            status: SessionStatus::Active,
            items: vec![answer, feedback],
        })
    }

    /// Ask the session's configured thinker for one text reply.
    async fn think(
        &self,
        session: &crate::domain::Session,
        task: ThinkerTask,
    ) -> Result<String, CoreError> {
        let reply = self.timeline.think(session, &task).await?;
        Ok(reply.text)
    }

    fn require_active(&self, status: SessionStatus, action: &str) -> Result<(), CoreError> {
        if status == SessionStatus::Active {
            Ok(())
        } else {
            Err(CoreError::InvalidLearningAction(format!(
                "{action} requires active, found {}",
                status.as_str()
            )))
        }
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
