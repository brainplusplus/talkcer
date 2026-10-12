//! PCM segmentation and speech-to-text selection.
//!
//! The live capture readers hand raw 16-bit little-endian PCM to this module.
//! [`Segmenter`] splits a buffer into speech runs using an energy threshold, so
//! a silent channel produces no utterances. [`Transcriber`] turns one speech run
//! into text. [`EmbeddedTranscriber`] is the offline default; a session can point
//! at a self-hosted or cloud endpoint through [`select_stt`] and
//! [`build_transcriber`], which [`HttpSttTranscriber`] then calls.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::ai::{AiConfiguration, ProviderConfig, ProviderKind};
use crate::domain::Session;
use crate::error::CoreError;

/// One run of speech inside a PCM buffer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Segment {
    /// Start time in milliseconds since the Unix epoch.
    pub started_at_ms: i64,
    /// End time in milliseconds since the Unix epoch.
    pub ended_at_ms: i64,
    /// Raw 16-bit little-endian PCM for the run.
    pub bytes: Vec<u8>,
    /// Peak window energy in the run.
    pub rms: u16,
    /// Sample rate of [`Self::bytes`], so a provider can wrap it in a container.
    pub sample_rate: u32,
    /// Channel count of [`Self::bytes`].
    pub channels: u16,
}

/// Root-mean-square amplitude of 16-bit little-endian PCM.
///
/// A trailing odd byte is ignored.
#[must_use]
pub fn rms_i16(bytes: &[u8]) -> u16 {
    let mut sum: u64 = 0;
    let mut count: u64 = 0;
    for pair in bytes.chunks_exact(2) {
        let sample = i64::from(i16::from_le_bytes([pair[0], pair[1]]));
        sum += (sample * sample) as u64;
        count += 1;
    }
    if count == 0 {
        return 0;
    }
    let mean = sum / count;
    (mean as f64).sqrt().round() as u16
}

/// Splits PCM into speech runs using an energy threshold.
#[derive(Debug, Clone, Copy)]
pub struct Segmenter {
    sample_rate: u32,
    channels: u16,
    window_bytes: usize,
    window_ms: i64,
    threshold: u16,
}

impl Segmenter {
    /// Build a segmenter. `window_ms` is the analysis window; `threshold` is the
    /// minimum RMS that counts as speech.
    #[must_use]
    pub fn new(sample_rate: u32, channels: u16, window_ms: u32, threshold: u16) -> Self {
        let frames = (u64::from(sample_rate) * u64::from(window_ms) / 1000).max(1);
        let window_bytes = (frames * u64::from(channels) * 2) as usize;
        Self {
            sample_rate,
            channels: channels.max(1),
            window_bytes: window_bytes.max(2),
            window_ms: i64::from(window_ms),
            threshold,
        }
    }

    /// Defaults for the 48 kHz stereo capture format.
    #[must_use]
    pub fn for_speech(sample_rate: u32, channels: u16) -> Self {
        Self::new(sample_rate, channels, 20, 200)
    }

    /// Duration of one analysis window in milliseconds.
    #[must_use]
    pub const fn window_ms(&self) -> i64 {
        self.window_ms
    }

    /// Number of analysis windows a buffer of `len` bytes spans.
    #[must_use]
    pub fn windows(&self, len: usize) -> i64 {
        let windows = len.div_ceil(self.window_bytes);
        windows as i64
    }

    /// Split `pcm` into speech runs. `start_ms` is the time of the first byte.
    #[must_use]
    pub fn segment(&self, start_ms: i64, pcm: &[u8]) -> Vec<Segment> {
        let mut segments = Vec::new();
        let mut current: Option<Segment> = None;
        let mut offset_ms = 0_i64;
        for window in pcm.chunks(self.window_bytes) {
            let rms = rms_i16(window);
            if rms >= self.threshold {
                match &mut current {
                    Some(segment) => {
                        segment.bytes.extend_from_slice(window);
                        segment.ended_at_ms = start_ms + offset_ms + self.window_ms;
                        segment.rms = segment.rms.max(rms);
                    }
                    None => {
                        current = Some(Segment {
                            started_at_ms: start_ms + offset_ms,
                            ended_at_ms: start_ms + offset_ms + self.window_ms,
                            bytes: window.to_vec(),
                            rms,
                            sample_rate: self.sample_rate,
                            channels: self.channels,
                        });
                    }
                }
            } else if let Some(segment) = current.take() {
                segments.push(segment);
            }
            offset_ms += self.window_ms;
        }
        if let Some(segment) = current {
            segments.push(segment);
        }
        segments
    }
}

/// Turns one speech run into text.
#[async_trait::async_trait]
pub trait Transcriber: Send + Sync + 'static {
    /// Return the transcript for a run, or `None` when it carries no text.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] when the backend fails.
    async fn transcribe(&self, segment: &Segment) -> Result<Option<String>, CoreError>;

    /// Registry name of the adapter, stored in transcript metadata.
    fn provider(&self) -> &'static str;
}

/// Offline adapter that decodes audio bytes carrying UTF-8 text.
///
/// This matches the mock voice contract and keeps tests deterministic. Real
/// microphone audio has no text, so it yields `None`; a self-hosted or cloud
/// endpoint is required for actual speech recognition.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmbeddedTranscriber;

#[async_trait::async_trait]
impl Transcriber for EmbeddedTranscriber {
    async fn transcribe(&self, segment: &Segment) -> Result<Option<String>, CoreError> {
        Ok(decode_text(&segment.bytes))
    }

    fn provider(&self) -> &'static str {
        "embedded-stt"
    }
}

/// Decode PCM bytes that carry UTF-8 text, rejecting binary audio.
#[must_use]
pub fn decode_text(bytes: &[u8]) -> Option<String> {
    let cleaned: Vec<u8> = bytes.iter().copied().filter(|byte| *byte != 0).collect();
    let text = String::from_utf8(cleaned).ok()?;
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let total = trimmed.chars().count();
    let printable = trimmed.chars().filter(|character| !character.is_control()).count();
    if printable * 2 < total {
        return None;
    }
    Some(trimmed.to_owned())
}

/// The STT choice a session resolved to, before the adapter is built.
///
/// This is the value returned by [`select_stt`]. It carries everything needed to
/// build a [`Transcriber`] plus the fields the UI shows.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct SttSelection {
    /// Registry provider name, such as `embedded-stt` or `cloud-stt`.
    pub provider: String,
    /// Deployment location.
    pub kind: ProviderKind,
    /// Model or voice identifier.
    pub model: String,
    /// Endpoint for self-hosted and cloud providers.
    pub endpoint: Option<String>,
    /// Credential name to look up at call time. Never the secret itself.
    pub credentials_ref: Option<String>,
    /// Language code sent to the provider.
    pub language: String,
}

impl SttSelection {
    /// The embedded offline choice.
    #[must_use]
    pub fn embedded() -> Self {
        Self {
            provider: "embedded-stt".to_owned(),
            kind: ProviderKind::Embedded,
            model: "embedded-utf8".to_owned(),
            endpoint: None,
            credentials_ref: None,
            language: "en".to_owned(),
        }
    }

    /// `true` when audio leaves this computer.
    #[must_use]
    pub const fn is_remote(&self) -> bool {
        matches!(self.kind, ProviderKind::SelfHosted | ProviderKind::Cloud)
    }
}

/// Resolve which STT provider a session should use.
///
/// Precedence:
/// 1. `session.configuration.stt` as a saved configuration id, when it names an
///    existing [`AiConfiguration`].
/// 2. The `stt` object inside `session.configuration`, when it is a provider
///    config.
/// 3. [`SttSelection::embedded`].
///
/// A remote selection is refused unless it carries consent, so audio cannot
/// leave the computer by accident. The returned value is validated against the
/// same registry rules the AI configuration area uses.
///
/// # Errors
///
/// Returns [`CoreError::InvalidConfiguration`] when the selected provider is
/// unknown, incomplete, or remote without consent.
pub fn select_stt(
    session: &Session,
    configurations: &[AiConfiguration],
) -> Result<SttSelection, CoreError> {
    let configured = session.configuration.get("stt");

    if let Some(Value::String(id)) = configured {
        if let Ok(id) = uuid::Uuid::parse_str(id) {
            if let Some(configuration) = configurations.iter().find(|item| item.id == id) {
                return selection_from_config(&configuration.stt);
            }
        }
    }

    if let Some(value) = configured.filter(|value| value.is_object()) {
        let config: ProviderConfig = serde_json::from_value(value.clone())
            .map_err(|error| CoreError::InvalidConfiguration(format!("stt config is invalid: {error}")))?;
        return selection_from_config(&config);
    }

    Ok(SttSelection::embedded())
}

/// Validate one stored STT slot and turn it into a selection.
///
/// # Errors
///
/// Returns [`CoreError::InvalidConfiguration`] for an unknown provider, a
/// missing endpoint or credential, or a remote provider without consent.
pub fn selection_from_config(config: &ProviderConfig) -> Result<SttSelection, CoreError> {
    let provider = config.provider.trim();
    let (known_kind, remote) = match provider {
        "embedded-stt" => (ProviderKind::Embedded, false),
        "cloud-stt" => (ProviderKind::Cloud, true),
        "self-hosted-stt" => (ProviderKind::SelfHosted, true),
        other => {
            return Err(CoreError::UnknownProvider(other.to_owned()));
        }
    };
    if config.kind != known_kind {
        return Err(CoreError::InvalidConfiguration(format!(
            "stt provider '{provider}' requires kind {}",
            known_kind.as_str()
        )));
    }
    let model = config.model.trim();
    if model.is_empty() {
        return Err(CoreError::InvalidConfiguration("stt model must not be empty".to_owned()));
    }
    let endpoint = config
        .endpoint
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if remote {
        let endpoint = endpoint.ok_or_else(|| {
            CoreError::InvalidConfiguration(format!("stt endpoint is required for {provider}"))
        })?;
        if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
            return Err(CoreError::InvalidConfiguration(
                "stt endpoint must start with http:// or https://".to_owned(),
            ));
        }
        if !config.consent {
            return Err(CoreError::InvalidConfiguration(
                "stt provider needs consent before session audio leaves this computer".to_owned(),
            ));
        }
    }
    if known_kind == ProviderKind::Cloud
        && config.credentials_ref.as_deref().map(str::trim).unwrap_or("").is_empty()
    {
        return Err(CoreError::InvalidConfiguration(
            "stt credentials_ref is required for cloud".to_owned(),
        ));
    }
    Ok(SttSelection {
        provider: provider.to_owned(),
        kind: known_kind,
        model: model.to_owned(),
        endpoint: endpoint.map(str::to_owned),
        credentials_ref: config.credentials_ref.clone(),
        language: "en".to_owned(),
    })
}

/// Build the adapter for a selection.
///
/// `credential` is the resolved secret for a cloud provider. It is read at call
/// time and never stored, so it does not appear in configuration or logs.
///
/// # Errors
///
/// Returns [`CoreError::InvalidConfiguration`] when a remote selection is
/// missing its endpoint, or when a cloud selection has no credential.
pub fn build_transcriber(
    selection: &SttSelection,
    credential: Option<String>,
) -> Result<Arc<dyn Transcriber>, CoreError> {
    match selection.kind {
        ProviderKind::Embedded => Ok(Arc::new(EmbeddedTranscriber)),
        ProviderKind::SelfHosted | ProviderKind::Cloud => {
            let endpoint = selection.endpoint.clone().ok_or_else(|| {
                CoreError::InvalidConfiguration(format!(
                    "stt endpoint is required for {}",
                    selection.provider
                ))
            })?;
            if selection.kind == ProviderKind::Cloud && credential.is_none() {
                return Err(CoreError::InvalidConfiguration(format!(
                    "credential '{}' is required for {}",
                    selection.credentials_ref.as_deref().unwrap_or(""),
                    selection.provider
                )));
            }
            Ok(Arc::new(HttpSttTranscriber {
                endpoint,
                model: selection.model.clone(),
                language: selection.language.clone(),
                credential,
            }))
        }
    }
}

/// Speech-to-text over an HTTP endpoint.
///
/// The request matches the widely used OpenAI transcription shape: a multipart
/// body with a WAV `file` and a `model` field, and a JSON `{"text": "..."}`
/// response. That covers a self-hosted whisper.cpp or faster-whisper server and
/// a cloud endpoint that speaks the same protocol.
#[derive(Debug, Clone)]
pub struct HttpSttTranscriber {
    endpoint: String,
    model: String,
    language: String,
    credential: Option<String>,
}

/// JSON body the adapter expects back.
#[derive(Debug, Deserialize)]
struct TranscriptionBody {
    text: String,
}

impl HttpSttTranscriber {
    /// Build an adapter for one endpoint.
    #[must_use]
    pub fn new(
        endpoint: impl Into<String>,
        model: impl Into<String>,
        language: impl Into<String>,
        credential: Option<String>,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            language: language.into(),
            credential,
        }
    }

    /// The URL a transcription request is posted to.
    #[must_use]
    pub fn request_url(&self) -> String {
        let base = self.endpoint.trim_end_matches('/');
        if base.ends_with("/audio/transcriptions") {
            base.to_owned()
        } else {
            format!("{base}/audio/transcriptions")
        }
    }
}

#[async_trait::async_trait]
impl Transcriber for HttpSttTranscriber {
    async fn transcribe(&self, segment: &Segment) -> Result<Option<String>, CoreError> {
        if segment.bytes.is_empty() {
            return Ok(None);
        }
        let wav = pcm16_to_wav(&segment.bytes, segment.sample_rate, segment.channels);
        let file = reqwest::multipart::Part::bytes(wav)
            .file_name("audio.wav")
            .mime_str("audio/wav")
            .map_err(|error| CoreError::Transcription(error.to_string()))?;
        let form = reqwest::multipart::Form::new()
            .part("file", file)
            .text("model", self.model.clone())
            .text("language", self.language.clone())
            .text("response_format", "json");
        let mut request = reqwest::Client::new().post(self.request_url()).multipart(form);
        if let Some(credential) = &self.credential {
            request = request.bearer_auth(credential);
        }
        let response = request.send().await.map_err(|error| {
            CoreError::Transcription(format!("stt request to {} failed: {error}", self.request_url()))
        })?;
        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(CoreError::Transcription(format!(
                "stt endpoint returned {status}: {}",
                detail.trim()
            )));
        }
        let body: TranscriptionBody = response
            .json()
            .await
            .map_err(|error| CoreError::Transcription(format!("stt response was not JSON: {error}")))?;
        let text = body.text.trim();
        if text.is_empty() {
            return Ok(None);
        }
        Ok(Some(text.to_owned()))
    }

    fn provider(&self) -> &'static str {
        "http-stt"
    }
}

/// Resolves the transcriber a session should use.
///
/// Capture asks this once per session before it drains audio, so a session can
/// run a different STT provider from the next one.
#[async_trait::async_trait]
pub trait SttResolver: Send + Sync + 'static {
    /// Build the transcriber for `session`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] when the selection is invalid or its credential is
    /// unavailable.
    async fn resolve(&self, session: &Session) -> Result<Arc<dyn Transcriber>, CoreError>;
}

/// Always returns the embedded offline transcriber.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmbeddedSttResolver;

#[async_trait::async_trait]
impl SttResolver for EmbeddedSttResolver {
    async fn resolve(&self, _session: &Session) -> Result<Arc<dyn Transcriber>, CoreError> {
        Ok(Arc::new(EmbeddedTranscriber))
    }
}

/// Always returns one fixed transcriber, ignoring the session.
///
/// Tests use this to drive the pipeline with a scripted adapter.
#[derive(Clone)]
pub struct FixedSttResolver {
    transcriber: Arc<dyn Transcriber>,
}

impl FixedSttResolver {
    /// Wrap one transcriber.
    #[must_use]
    pub fn new(transcriber: Arc<dyn Transcriber>) -> Self {
        Self { transcriber }
    }
}

#[async_trait::async_trait]
impl SttResolver for FixedSttResolver {
    async fn resolve(&self, _session: &Session) -> Result<Arc<dyn Transcriber>, CoreError> {
        Ok(Arc::clone(&self.transcriber))
    }
}

/// Resolve a credential by name from the process environment.
///
/// The name is uppercased and prefixed with `TALKCER_SECRET_`, so a
/// `credentials_ref` of `openai-prod` reads `TALKCER_SECRET_OPENAI_PROD`. The
/// value is never stored, logged, or returned to a client.
#[must_use]
pub fn credential_from_env(credentials_ref: &str) -> Option<String> {
    let sanitized: String = credentials_ref
        .trim()
        .to_ascii_uppercase()
        .chars()
        .map(|character| if character.is_ascii_alphanumeric() { character } else { '_' })
        .collect();
    let key = format!("TALKCER_SECRET_{sanitized}");
    std::env::var(key).ok().filter(|value| !value.trim().is_empty())
}

/// Resolves a session's STT provider from saved AI configurations.
///
/// This is the production resolver. It reads every configuration, lets
/// [`select_stt`] pick one for the session, resolves the credential from the
/// environment, and builds the adapter.
#[derive(Clone)]
pub struct ConfiguredSttResolver {
    repository: Arc<dyn crate::ai::AiConfigurationRepository>,
}

impl ConfiguredSttResolver {
    /// Wrap an AI configuration repository.
    #[must_use]
    pub fn new(repository: Arc<dyn crate::ai::AiConfigurationRepository>) -> Self {
        Self { repository }
    }
}

#[async_trait::async_trait]
impl SttResolver for ConfiguredSttResolver {
    async fn resolve(&self, session: &Session) -> Result<Arc<dyn Transcriber>, CoreError> {
        let configurations = self.repository.list().await?;
        let selection = select_stt(session, &configurations)?;
        let credential = selection
            .credentials_ref
            .as_deref()
            .and_then(credential_from_env);
        build_transcriber(&selection, credential)
    }
}

/// Wrap 16-bit little-endian PCM in a minimal WAV container.
#[must_use]
pub fn pcm16_to_wav(pcm: &[u8], sample_rate: u32, channels: u16) -> Vec<u8> {
    let channels = channels.max(1);
    let bits = 16_u16;
    let block_align = channels * bits / 8;
    let byte_rate = sample_rate * u32::from(block_align);
    let data_len = pcm.len() as u32;
    let mut wav = Vec::with_capacity(44 + pcm.len());
    wav.extend_from_slice(b"RIFF");
    wav.extend_from_slice(&(36 + data_len).to_le_bytes());
    wav.extend_from_slice(b"WAVE");
    wav.extend_from_slice(b"fmt ");
    wav.extend_from_slice(&16_u32.to_le_bytes());
    wav.extend_from_slice(&1_u16.to_le_bytes()); // PCM
    wav.extend_from_slice(&channels.to_le_bytes());
    wav.extend_from_slice(&sample_rate.to_le_bytes());
    wav.extend_from_slice(&byte_rate.to_le_bytes());
    wav.extend_from_slice(&block_align.to_le_bytes());
    wav.extend_from_slice(&bits.to_le_bytes());
    wav.extend_from_slice(b"data");
    wav.extend_from_slice(&data_len.to_le_bytes());
    wav.extend_from_slice(pcm);
    wav
}
