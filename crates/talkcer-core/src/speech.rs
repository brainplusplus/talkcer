//! Text-to-speech selection and adapters.
//!
//! TTS stays gated by `session.configuration.tts == "enabled"`, exactly as
//! before. The provider is chosen separately from
//! `session.configuration.tts_provider`, so turning speech on and picking where
//! it runs stay independent. [`EmbeddedTts`] is the offline tone generator;
//! [`HttpTts`] posts to an OpenAI-compatible `/audio/speech` endpoint.

use std::sync::Arc;

use serde_json::json;

use crate::ai::{AiConfiguration, ProviderConfig, ProviderKind};
use crate::domain::Session;
use crate::error::CoreError;

/// Synthesized audio returned to the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SpeechOutput {
    /// MIME type of [`Self::bytes`].
    pub content_type: String,
    /// Raw audio bytes.
    pub bytes: Vec<u8>,
}

/// Turns text into audio.
#[async_trait::async_trait]
pub trait Tts: Send + Sync + 'static {
    /// Synthesize one string.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] when the backend fails.
    async fn synthesize(&self, text: &str) -> Result<SpeechOutput, CoreError>;

    /// Registry name of the adapter.
    fn provider(&self) -> &'static str;
}

/// Offline tone generator. Deterministic and network-free.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmbeddedTts;

#[async_trait::async_trait]
impl Tts for EmbeddedTts {
    async fn synthesize(&self, text: &str) -> Result<SpeechOutput, CoreError> {
        Ok(SpeechOutput {
            content_type: "audio/wav".to_owned(),
            bytes: tone_wav(text),
        })
    }

    fn provider(&self) -> &'static str {
        "embedded-tts"
    }
}

/// Build a short 16-bit mono WAV whose tone depends on the text length.
#[must_use]
pub fn tone_wav(text: &str) -> Vec<u8> {
    let samples = 800_u32;
    let mut body = Vec::with_capacity(samples as usize * 2);
    let scale = (text.len() % 12 + 1) as f32;
    for index in 0..samples {
        let angle = index as f32 * scale * 0.05;
        let sample = (angle.sin() * 3000.0) as i16;
        body.extend(sample.to_le_bytes());
    }
    let data_size = body.len() as u32;
    let mut wav = Vec::with_capacity(44 + body.len());
    wav.extend(b"RIFF");
    wav.extend((36 + data_size).to_le_bytes());
    wav.extend(b"WAVEfmt ");
    wav.extend(16_u32.to_le_bytes());
    wav.extend(1_u16.to_le_bytes());
    wav.extend(1_u16.to_le_bytes());
    wav.extend(8_000_u32.to_le_bytes());
    wav.extend(16_000_u32.to_le_bytes());
    wav.extend(2_u16.to_le_bytes());
    wav.extend(16_u16.to_le_bytes());
    wav.extend(b"data");
    wav.extend(data_size.to_le_bytes());
    wav.extend(body);
    wav
}

/// TTS over an OpenAI-compatible speech endpoint.
#[derive(Debug, Clone)]
pub struct HttpTts {
    endpoint: String,
    model: String,
    voice: String,
    credential: Option<String>,
}

impl HttpTts {
    /// Build an adapter for one endpoint.
    #[must_use]
    pub fn new(
        endpoint: impl Into<String>,
        model: impl Into<String>,
        voice: impl Into<String>,
        credential: Option<String>,
    ) -> Self {
        Self {
            endpoint: endpoint.into(),
            model: model.into(),
            voice: voice.into(),
            credential,
        }
    }

    /// The URL a synthesis request is posted to.
    #[must_use]
    pub fn request_url(&self) -> String {
        let base = self.endpoint.trim_end_matches('/');
        if base.ends_with("/audio/speech") {
            base.to_owned()
        } else {
            format!("{base}/audio/speech")
        }
    }

    /// The JSON body for one string.
    #[must_use]
    pub fn request_body(&self, text: &str) -> serde_json::Value {
        json!({
            "model": self.model,
            "input": text,
            "voice": self.voice,
            "response_format": "wav",
        })
    }
}

#[async_trait::async_trait]
impl Tts for HttpTts {
    async fn synthesize(&self, text: &str) -> Result<SpeechOutput, CoreError> {
        let mut request = reqwest::Client::new()
            .post(self.request_url())
            .json(&self.request_body(text));
        if let Some(credential) = &self.credential {
            request = request.bearer_auth(credential);
        }
        let response = request.send().await.map_err(|error| {
            CoreError::ProviderUnavailable(format!(
                "tts request to {} failed: {error}",
                self.request_url()
            ))
        })?;
        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(CoreError::ProviderUnavailable(format!(
                "tts endpoint returned {status}: {}",
                detail.trim()
            )));
        }
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned)
            .unwrap_or_else(|| "audio/wav".to_owned());
        let bytes = response
            .bytes()
            .await
            .map_err(|error| {
                CoreError::ProviderUnavailable(format!("tts response body failed: {error}"))
            })?
            .to_vec();
        if bytes.is_empty() {
            return Err(CoreError::ProviderUnavailable(
                "tts endpoint returned no audio".to_owned(),
            ));
        }
        Ok(SpeechOutput {
            content_type,
            bytes,
        })
    }

    fn provider(&self) -> &'static str {
        "http-tts"
    }
}

/// The TTS choice a session resolved to.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct TtsSelection {
    /// Registry provider name.
    pub provider: String,
    /// Deployment location.
    pub kind: ProviderKind,
    /// Model identifier.
    pub model: String,
    /// Endpoint for self-hosted and cloud providers.
    pub endpoint: Option<String>,
    /// Credential name to look up at call time.
    pub credentials_ref: Option<String>,
    /// Voice identifier sent to the provider.
    pub voice: String,
}

impl TtsSelection {
    /// The embedded offline choice.
    #[must_use]
    pub fn embedded() -> Self {
        Self {
            provider: "embedded-tts".to_owned(),
            kind: ProviderKind::Embedded,
            model: "embedded-tone".to_owned(),
            endpoint: None,
            credentials_ref: None,
            voice: "default".to_owned(),
        }
    }

    /// `true` when text leaves this computer for speech.
    #[must_use]
    pub const fn is_remote(&self) -> bool {
        matches!(self.kind, ProviderKind::SelfHosted | ProviderKind::Cloud)
    }
}

/// Resolve which TTS provider a session should use.
///
/// Precedence: saved configuration id, then an inline
/// `session.configuration.tts_provider` object, then [`TtsSelection::embedded`].
///
/// # Errors
///
/// Returns [`CoreError::InvalidConfiguration`] when the selection is unknown,
/// incomplete, or remote without consent.
pub fn select_tts(
    session: &Session,
    configurations: &[AiConfiguration],
) -> Result<TtsSelection, CoreError> {
    let configured = session.configuration.get("tts_provider");
    if let Some(serde_json::Value::String(id)) = configured {
        if let Ok(id) = uuid::Uuid::parse_str(id) {
            if let Some(configuration) = configurations.iter().find(|item| item.id == id) {
                return tts_from_config(&configuration.tts);
            }
        }
    }
    if let Some(value) = configured.filter(|value| value.is_object()) {
        let config: ProviderConfig = serde_json::from_value(value.clone()).map_err(|error| {
            CoreError::InvalidConfiguration(format!("tts config is invalid: {error}"))
        })?;
        return tts_from_config(&config);
    }
    // The session page stores its pick as `provider:model` under `tts_model`.
    if let Some(serde_json::Value::String(choice)) = session.configuration.get("tts_model") {
        if !choice.trim().is_empty() {
            return tts_for_model(choice, configurations);
        }
    }
    Ok(TtsSelection::embedded())
}

/// Turn a `provider:model` pick into a TTS selection.
///
/// The endpoint, credential, and consent come from the first saved
/// configuration for that provider, so the picker does not duplicate them.
///
/// # Errors
///
/// Returns [`CoreError::InvalidConfiguration`] when the value is not
/// `provider:model` or the provider has no saved configuration, and
/// [`CoreError::UnknownProvider`] for an unknown provider.
pub fn tts_for_model(
    choice: &str,
    configurations: &[AiConfiguration],
) -> Result<TtsSelection, CoreError> {
    let (provider, model) = choice.split_once(':').ok_or_else(|| {
        CoreError::InvalidConfiguration(format!(
            "tts model '{choice}' must be provider:model"
        ))
    })?;
    let provider = provider.trim();
    let model = model.trim();
    if provider.is_empty() || model.is_empty() {
        return Err(CoreError::InvalidConfiguration(format!(
            "tts model '{choice}' must name a provider and a model"
        )));
    }
    let kind = crate::ai::provider_kind(crate::ai::Capability::Tts, provider)
        .ok_or_else(|| CoreError::UnknownProvider(provider.to_owned()))?;
    if let Some(config) = configurations
        .iter()
        .map(|item| &item.tts)
        .find(|item| item.provider.trim() == provider)
    {
        return tts_from_config(&ProviderConfig {
            model: model.to_owned(),
            ..config.clone()
        });
    }
    if kind != ProviderKind::Embedded {
        return Err(CoreError::InvalidConfiguration(format!(
            "'{provider}' needs a saved configuration before it can speak"
        )));
    }
    Ok(TtsSelection {
        provider: provider.to_owned(),
        kind,
        model: model.to_owned(),
        endpoint: None,
        credentials_ref: None,
        voice: "default".to_owned(),
    })
}

/// Validate one stored TTS slot.
///
/// # Errors
///
/// Returns [`CoreError`] for an unknown provider or an incomplete remote slot.
pub fn tts_from_config(config: &ProviderConfig) -> Result<TtsSelection, CoreError> {
    let provider = config.provider.trim();
    // Both the legacy slot names and the catalog provider ids are accepted, so
    // a voice picked as `tts-openai:alloy` validates like a saved slot does.
    let known_kind = match provider {
        "embedded-tts" | "disabled-tts" => ProviderKind::Embedded,
        "cloud-tts" => ProviderKind::Cloud,
        "self-hosted-tts" => ProviderKind::SelfHosted,
        other => crate::ai::provider_kind(crate::ai::Capability::Tts, other)
            .ok_or_else(|| CoreError::UnknownProvider(other.to_owned()))?,
    };
    let remote = matches!(
        known_kind,
        ProviderKind::SelfHosted | ProviderKind::Cloud
    );
    if config.kind != known_kind {
        return Err(CoreError::InvalidConfiguration(format!(
            "tts provider '{provider}' requires kind {}",
            known_kind.as_str()
        )));
    }
    let model = config.model.trim();
    if model.is_empty() {
        return Err(CoreError::InvalidConfiguration(
            "tts model must not be empty".to_owned(),
        ));
    }
    let endpoint = config
        .endpoint
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    if remote {
        let endpoint = endpoint.ok_or_else(|| {
            CoreError::InvalidConfiguration(format!("tts endpoint is required for {provider}"))
        })?;
        if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
            return Err(CoreError::InvalidConfiguration(
                "tts endpoint must start with http:// or https://".to_owned(),
            ));
        }
        if !config.consent {
            return Err(CoreError::InvalidConfiguration(
                "tts provider needs consent before session text leaves this computer".to_owned(),
            ));
        }
    }
    if known_kind == ProviderKind::Cloud
        && config
            .credentials_ref
            .as_deref()
            .map(str::trim)
            .unwrap_or("")
            .is_empty()
    {
        return Err(CoreError::InvalidConfiguration(
            "tts credentials_ref is required for cloud".to_owned(),
        ));
    }
    Ok(TtsSelection {
        provider: provider.to_owned(),
        kind: known_kind,
        model: model.to_owned(),
        endpoint: endpoint.map(str::to_owned),
        credentials_ref: config.credentials_ref.clone(),
        voice: "default".to_owned(),
    })
}

/// Build the adapter for a selection.
///
/// # Errors
///
/// Returns [`CoreError::InvalidConfiguration`] when a remote selection is
/// incomplete or a cloud selection has no credential.
pub fn build_tts(
    selection: &TtsSelection,
    credential: Option<String>,
) -> Result<Arc<dyn Tts>, CoreError> {
    match selection.kind {
        ProviderKind::Embedded => Ok(Arc::new(EmbeddedTts)),
        ProviderKind::SelfHosted | ProviderKind::Cloud => {
            let endpoint = selection.endpoint.clone().ok_or_else(|| {
                CoreError::InvalidConfiguration(format!(
                    "tts endpoint is required for {}",
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
            Ok(Arc::new(HttpTts {
                endpoint,
                model: selection.model.clone(),
                voice: selection.voice.clone(),
                credential,
            }))
        }
    }
}

/// Resolves the TTS provider a session should use.
#[async_trait::async_trait]
pub trait TtsResolver: Send + Sync + 'static {
    /// Build the TTS adapter for `session`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] when the selection is invalid or its credential is
    /// unavailable.
    async fn resolve(&self, session: &Session) -> Result<Arc<dyn Tts>, CoreError>;
}

/// Always returns the embedded offline tone generator.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmbeddedTtsResolver;

#[async_trait::async_trait]
impl TtsResolver for EmbeddedTtsResolver {
    async fn resolve(&self, _session: &Session) -> Result<Arc<dyn Tts>, CoreError> {
        Ok(Arc::new(EmbeddedTts))
    }
}

/// Always returns one fixed adapter, ignoring the session.
#[derive(Clone)]
pub struct FixedTtsResolver {
    tts: Arc<dyn Tts>,
}

impl FixedTtsResolver {
    /// Wrap one adapter.
    #[must_use]
    pub fn new(tts: Arc<dyn Tts>) -> Self {
        Self { tts }
    }
}

#[async_trait::async_trait]
impl TtsResolver for FixedTtsResolver {
    async fn resolve(&self, _session: &Session) -> Result<Arc<dyn Tts>, CoreError> {
        Ok(Arc::clone(&self.tts))
    }
}

/// Resolves a session's TTS provider from saved AI configurations.
#[derive(Clone)]
pub struct ConfiguredTtsResolver {
    repository: Arc<dyn crate::ai::AiConfigurationRepository>,
}

impl ConfiguredTtsResolver {
    /// Wrap an AI configuration repository.
    #[must_use]
    pub fn new(repository: Arc<dyn crate::ai::AiConfigurationRepository>) -> Self {
        Self { repository }
    }
}

#[async_trait::async_trait]
impl TtsResolver for ConfiguredTtsResolver {
    async fn resolve(&self, session: &Session) -> Result<Arc<dyn Tts>, CoreError> {
        let configurations = self.repository.list().await?;
        let selection = select_tts(session, &configurations)?;
        let credential = selection
            .credentials_ref
            .as_deref()
            .and_then(crate::audio::credential_from_env);
        build_tts(&selection, credential)
    }
}

/// Parse a provider config from a JSON object, for tests and callers.
///
/// # Errors
///
/// Returns [`CoreError::InvalidConfiguration`] when the object is not a config.
pub fn provider_config_from_value(value: &serde_json::Value) -> Result<ProviderConfig, CoreError> {
    serde_json::from_value(value.clone()).map_err(|error| {
        CoreError::InvalidConfiguration(format!("provider config is invalid: {error}"))
    })
}
