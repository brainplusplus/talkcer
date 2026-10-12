//! Browser voice adapters.
//!
//! The mock STT adapter accepts text or UTF-8 audio chunks so tests do not
//! need a microphone. Browser capture is stored as `microphone` on the
//! timeline and `browser_mic` in metadata. TTS is resolved per session: the
//! embedded adapter returns a local WAV, and a saved configuration can point at
//! an OpenAI-compatible speech endpoint.

use std::sync::Arc;

use base64::Engine;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use uuid::Uuid;

use crate::domain::Session;
use crate::error::CoreError;
use crate::session::SessionRepository;
use crate::timeline::{Source, Speaker, TimelineRepository, TimelineService, UpsertTranscript};

/// One mock audio chunk.
#[derive(Debug, Clone, Deserialize)]
pub struct VoiceAudio {
    /// Existing utterance for a later partial or final.
    #[serde(default)]
    pub utterance_id: Option<Uuid>,
    /// Capture origin. Browser audio uses `browser_mic`.
    pub audio_source: String,
    /// Optional scripted transcript used by the mock adapter.
    #[serde(default)]
    pub text: Option<String>,
    /// Optional base64 chunks. UTF-8 bytes are the mock transcript.
    #[serde(default)]
    pub chunks: Vec<String>,
    /// Ordering timestamp.
    pub started_at_ms: i64,
    /// Set on the closing chunk.
    #[serde(default)]
    pub is_final: bool,
}

/// Playback request for one AI response.
#[derive(Debug, Clone, Deserialize)]
pub struct SpeakRequest {
    /// Text to speak.
    pub text: String,
}

/// Local TTS result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SpeechAudio {
    /// `false` when session configuration disables TTS.
    pub enabled: bool,
    /// MIME type when audio is present.
    pub content_type: Option<String>,
    /// Base64 WAV when TTS is enabled.
    pub audio_base64: Option<String>,
}

/// Streaming mock transcript event before it is stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MockSttEvent {
    /// Partial or final text accumulated so far.
    pub text: String,
    /// `false` until the caller closes the utterance.
    pub is_final: bool,
}

/// Mock speech-to-text and configurable speech synthesis.
#[derive(Clone)]
pub struct VoiceService<S, T> {
    sessions: crate::session::SessionService<S>,
    timeline: TimelineService<T>,
    tts: Arc<dyn crate::speech::TtsResolver>,
}

impl<S, T> VoiceService<S, T>
where
    S: SessionRepository,
    T: TimelineRepository,
{
    /// Build a voice service over the existing stores with the embedded TTS.
    #[must_use]
    pub fn new(sessions: crate::session::SessionService<S>, timeline: TimelineService<T>) -> Self {
        Self {
            sessions,
            timeline,
            tts: Arc::new(crate::speech::EmbeddedTtsResolver),
        }
    }

    /// Build a voice service that resolves TTS from the session configuration.
    #[must_use]
    pub fn with_tts(
        sessions: crate::session::SessionService<S>,
        timeline: TimelineService<T>,
        tts: Arc<dyn crate::speech::TtsResolver>,
    ) -> Self {
        Self {
            sessions,
            timeline,
            tts,
        }
    }

    /// Convert one mock audio update into a transcript row.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidAudio`] for an unknown source or undecodable
    /// chunk, and the transcript errors from the timeline store.
    pub async fn transcribe(
        &self,
        session_id: Uuid,
        input: VoiceAudio,
    ) -> Result<crate::timeline::TranscriptUpsert, CoreError> {
        self.sessions.get(session_id).await?;
        if input.audio_source != "browser_mic" && input.audio_source != "mock_audio" {
            return Err(CoreError::InvalidAudio(
                "audio_source must be browser_mic or mock_audio".to_owned(),
            ));
        }
        let text = transcript_text(&input)?;
        let event = mock_stt_event(&text, input.is_final);
        self.timeline
            .upsert_transcript(
                session_id,
                UpsertTranscript {
                    utterance_id: input.utterance_id,
                    speaker: Speaker::LocalUser,
                    source: Source::Microphone,
                    text: event.text,
                    started_at_ms: input.started_at_ms,
                    ended_at_ms: input.is_final.then_some(input.started_at_ms.saturating_add(1)),
                    is_final: event.is_final,
                    metadata: Some(json!({ "audio_source": input.audio_source })),
                },
            )
            .await
    }

    /// Synthesize speech when the session configuration enables TTS.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::NotFound`] when the session does not exist and
    /// [`CoreError::EmptyText`] when the text is blank while TTS is enabled.
    pub async fn speak(&self, session_id: Uuid, request: SpeakRequest) -> Result<SpeechAudio, CoreError> {
        let session = self.sessions.get(session_id).await?;
        if !tts_enabled(&session) {
            return Ok(SpeechAudio {
                enabled: false,
                content_type: None,
                audio_base64: None,
            });
        }
        let text = request.text.trim();
        if text.is_empty() {
            return Err(CoreError::EmptyText);
        }
        let tts = self.tts.resolve(&session).await?;
        let output = tts.synthesize(text).await?;
        Ok(SpeechAudio {
            enabled: true,
            content_type: Some(output.content_type),
            audio_base64: Some(
                base64::engine::general_purpose::STANDARD.encode(output.bytes),
            ),
        })
    }
}

/// Decode the mock adapter input into the text accumulated so far.
///
/// # Errors
///
/// Returns [`CoreError::InvalidAudio`] when neither text nor a decodable chunk is present.
pub fn transcript_text(input: &VoiceAudio) -> Result<String, CoreError> {
    if let Some(text) = &input.text {
        let text = text.trim().to_owned();
        if !text.is_empty() {
            return Ok(text);
        }
    }
    let mut collected = String::new();
    for chunk in &input.chunks {
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(chunk.trim())
            .map_err(|_| CoreError::InvalidAudio("chunk is not base64".to_owned()))?;
        let text = String::from_utf8(bytes)
            .map_err(|_| CoreError::InvalidAudio("chunk is not UTF-8 text".to_owned()))?;
        collected.push_str(&text);
    }
    let collected = collected.trim().to_owned();
    if collected.is_empty() {
        Err(CoreError::InvalidAudio("mock audio did not contain text".to_owned()))
    } else {
        Ok(collected)
    }
}

/// The mock adapter emits the accumulated text and its final flag.
#[must_use]
pub fn mock_stt_event(text: &str, is_final: bool) -> MockSttEvent {
    MockSttEvent {
        text: text.to_owned(),
        is_final,
    }
}

fn tts_enabled(session: &Session) -> bool {
    session.configuration.get("tts").and_then(Value::as_str) == Some("enabled")
}

