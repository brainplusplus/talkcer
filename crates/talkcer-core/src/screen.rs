//! Full-screen capture as context for the selected mode.
//!
//! The capture always covers the whole current screen. There is no window
//! picker: the point is to read whatever a screen share is showing, such as a
//! question presented in Zoom, and relate it to the running mode.
//!
//! Two paths share one capture and differ only in when a model is called.
//!
//! - **Direct.** A vision model reads the frame, then the session's thinker
//!   answers from that reading, guided by the mode.
//! - **Staged.** The frame is stored on the timeline and nothing is sent. The
//!   user can capture several times and add a note, then send them together.

use std::sync::Arc;

use chrono::Utc;
use serde_json::{json, Value};
use uuid::Uuid;

use crate::domain::SessionMode;
use crate::error::CoreError;
use crate::thinker::{Thinker, ThinkerReply};
use crate::timeline::{ItemKind, Purpose, Source, Speaker, TimelineItem, TimelineService};

/// One full-screen frame.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenFrame {
    /// PNG bytes of the whole current screen.
    pub png: Vec<u8>,
    /// Width in pixels.
    pub width: u32,
    /// Height in pixels.
    pub height: u32,
}

/// Produces one full-screen frame.
///
/// The Windows implementation uses Graphics Capture. Tests supply a frame
/// directly, so the two paths are covered without a display.
#[async_trait::async_trait]
pub trait ScreenCapturer: Send + Sync {
    /// Capture the whole current screen.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Screen`] when no frame can be captured.
    async fn capture(&self) -> Result<ScreenFrame, CoreError>;
}

/// A vision reader over an OpenAI-compatible chat endpoint.
///
/// The frame goes up as an image part and the model answers with text. That text
/// is what the thinker then reasons about, so the rest of the pipeline stays
/// text-only.
#[derive(Debug, Clone)]
pub struct HttpVision {
    endpoint: String,
    model: String,
    credential: Option<String>,
}

impl HttpVision {
    /// Build a reader for one endpoint.
    #[must_use]
    pub fn new(
        endpoint: impl Into<String>,
        model: impl Into<String>,
        credential: Option<String>,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            credential,
        }
    }

    fn request_url(&self) -> String {
        format!("{}/chat/completions", self.endpoint.trim_end_matches('/'))
    }
}

#[async_trait::async_trait]
impl VisionReader for HttpVision {
    async fn read(&self, frame: &ScreenFrame) -> Result<String, CoreError> {
        let encoded =
            base64::Engine::encode(&base64::engine::general_purpose::STANDARD, &frame.png);
        let body = serde_json::json!({
            "model": self.model,
            "messages": [{
                "role": "user",
                "content": [
                    { "type": "text", "text": "Read the screen. Transcribe every question and the visible answer choices verbatim, then describe anything else shown." },
                    { "type": "image_url", "image_url": { "url": format!("data:image/png;base64,{encoded}") } }
                ]
            }]
        });
        let mut request = reqwest::Client::new().post(self.request_url()).json(&body);
        if let Some(credential) = &self.credential {
            request = request.bearer_auth(credential);
        }
        let response = request.send().await.map_err(|error| {
            CoreError::Screen(format!(
                "vision request to {} failed: {error}",
                self.request_url()
            ))
        })?;
        if !response.status().is_success() {
            let status = response.status();
            let detail = response.text().await.unwrap_or_default();
            return Err(CoreError::Screen(format!(
                "vision endpoint returned {status}: {}",
                detail.trim()
            )));
        }
        let body: serde_json::Value = response
            .json()
            .await
            .map_err(|error| CoreError::Screen(format!("vision response was not JSON: {error}")))?;
        let text = body["choices"][0]["message"]["content"]
            .as_str()
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .ok_or_else(|| CoreError::Screen("vision model returned no content".to_owned()))?;
        Ok(text.to_owned())
    }
}

/// Reads a frame into text.
///
/// A vision model does this for real. The reading is plain text so the thinker,
/// which only accepts text, can reason about the screen.
#[async_trait::async_trait]
pub trait VisionReader: Send + Sync {
    /// Describe what the frame shows.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Screen`] when the model cannot read the frame.
    async fn read(&self, frame: &ScreenFrame) -> Result<String, CoreError>;
}

/// What to do with a captured screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScreenIntent {
    /// Read the screen and answer immediately, using the mode as context.
    Direct,
    /// Store the screen and wait for an explicit send.
    Stage,
}

/// The outcome of handling one screen capture.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenOutcome {
    /// The reading or the staged note.
    pub text: String,
    /// The assistant reply, present only for [`ScreenIntent::Direct`].
    pub reply: Option<TimelineItem>,
    /// The row that holds the captured screen.
    pub captured: TimelineItem,
}

/// A reading plus the answer a mode produced from it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScreenAnswer {
    /// What the vision reader saw.
    pub reading: String,
    /// The thinker's reply.
    pub reply: ThinkerReply,
}

/// Turn one screen reading into an answer that fits the session mode.
///
/// The mode decides the stance: a live interview gets a concise answer, a mock
/// interview gets a question back, and learning gets an explanation.
///
/// # Errors
///
/// Returns the thinker's error when it cannot answer.
pub async fn answer_from_screen(
    thinker: &Arc<dyn Thinker>,
    mode: SessionMode,
    reading: &str,
    note: Option<&str>,
) -> Result<ScreenAnswer, CoreError> {
    let reply = thinker.complete(&screen_task(mode, reading, note)).await?;
    Ok(ScreenAnswer {
        reading: reading.to_owned(),
        reply,
    })
}

/// The thinker task for one screen reading, phrased for the mode.
pub fn screen_task(
    mode: SessionMode,
    reading: &str,
    note: Option<&str>,
) -> crate::thinker::ThinkerTask {
    let extra = note
        .map(|text| format!(" The user adds: {text}"))
        .unwrap_or_default();
    let prompt = match mode {
        SessionMode::LiveInterview => format!(
            "A screen share shows: {reading}.{extra} Answer the question it asks in at most three sentences."
        ),
        SessionMode::MockInterview => format!(
            "A screen share shows: {reading}.{extra} Ask one mock-interview question about what it shows."
        ),
        SessionMode::Learning => format!(
            "A screen share shows: {reading}.{extra} Explain what it shows and how it relates to the topic."
        ),
    };
    crate::thinker::ThinkerTask::ScreenContext { prompt }
}

/// Store one captured screen and, for the direct path, the answer it produced.
///
/// # Errors
///
/// Returns a database error when a row cannot be stored.
pub async fn record_screen<T>(
    timeline: &TimelineService<T>,
    session_id: Uuid,
    intent: ScreenIntent,
    frame: &ScreenFrame,
    reading: Option<&str>,
    reply_text: Option<&str>,
) -> Result<ScreenOutcome, CoreError>
where
    T: crate::timeline::TimelineRepository,
{
    let pending = intent == ScreenIntent::Stage;
    let captured = timeline
        .add_item(session_item(
            session_id,
            ItemKind::SystemEvent,
            Speaker::System,
            reading.unwrap_or("Screen captured, waiting to send."),
            json!({
                "screen": true,
                "pending": pending,
                "width": frame.width,
                "height": frame.height,
            }),
        ))
        .await?;
    let reply = match reply_text {
        Some(text) => Some(
            timeline
                .add_item(session_item(
                    session_id,
                    ItemKind::AiMessage,
                    Speaker::Ai,
                    text,
                    json!({ "screen": true, "from": captured.id }),
                ))
                .await?,
        ),
        None => None,
    };
    Ok(ScreenOutcome {
        text: reading.unwrap_or("").to_owned(),
        reply,
        captured,
    })
}

/// Collect the staged screen rows for one session, oldest first.
///
/// A row is staged when it carries `screen: true` and `pending: true`. Sending
/// them clears the flag, so a later send does not repeat them.
pub fn pending_screens(items: &[TimelineItem]) -> Vec<&TimelineItem> {
    items.iter().filter(|item| is_pending(item)).collect()
}

/// Whether a row is a staged screen that has not been sent.
pub fn is_pending(item: &TimelineItem) -> bool {
    item.metadata.get("screen").and_then(Value::as_bool) == Some(true)
        && item.metadata.get("pending").and_then(Value::as_bool) == Some(true)
}

/// Mark staged rows as sent so the next send starts clean.
pub fn mark_sent(metadata: &mut Value) {
    if let Some(object) = metadata.as_object_mut() {
        object.insert("pending".to_owned(), json!(false));
    }
}

fn session_item(
    session_id: Uuid,
    kind: ItemKind,
    speaker: Speaker,
    text: &str,
    metadata: Value,
) -> TimelineItem {
    TimelineItem {
        id: Uuid::now_v7(),
        session_id,
        kind,
        speaker,
        source: Source::Text,
        purpose: Purpose::Conversation,
        text: text.to_owned(),
        started_at_ms: Utc::now().timestamp_millis(),
        ended_at_ms: None,
        is_final: true,
        metadata,
        created_at: Utc::now(),
        utterance_id: None,
    }
}
