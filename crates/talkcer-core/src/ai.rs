//! AI configuration, the provider registry, and provider tests.
//!
//! Configuration stores a credential reference, never an API key. The registry
//! describes capabilities so the UI does not depend on one provider.

use std::sync::Arc;

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::error::CoreError;

const CONFIG_COLUMNS: &str =
    "id, name, stt_config, thinker_config, tts_config, created_at, updated_at";

/// Where a capability runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderKind {
    /// A process-local implementation.
    Embedded,
    /// An endpoint operated by the user.
    SelfHosted,
    /// A remote vendor endpoint.
    Cloud,
}

impl ProviderKind {
    /// Parse a wire kind.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidProviderKind`] for an unknown value.
    pub fn parse(value: &str) -> Result<Self, CoreError> {
        match value {
            "embedded" => Ok(Self::Embedded),
            "self_hosted" => Ok(Self::SelfHosted),
            "cloud" => Ok(Self::Cloud),
            other => Err(CoreError::InvalidProviderKind(other.to_owned())),
        }
    }

    /// Wire value stored in JSON.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Embedded => "embedded",
            Self::SelfHosted => "self_hosted",
            Self::Cloud => "cloud",
        }
    }
}

/// Which AI capability a configuration slot or test targets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    /// Speech to text.
    Stt,
    /// The language model.
    Thinker,
    /// Text to speech.
    Tts,
}

impl Capability {
    /// Parse a wire capability.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidCapability`] for an unknown value.
    pub fn parse(value: &str) -> Result<Self, CoreError> {
        match value {
            "stt" => Ok(Self::Stt),
            "thinker" => Ok(Self::Thinker),
            "tts" => Ok(Self::Tts),
            other => Err(CoreError::InvalidCapability(other.to_owned())),
        }
    }

    /// Wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Stt => "stt",
            Self::Thinker => "thinker",
            Self::Tts => "tts",
        }
    }
}

/// Configuration for one capability.
///
/// `credentials_ref` names a secret stored outside the database. The value
/// itself is never accepted here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderConfig {
    /// Deployment location.
    pub kind: ProviderKind,
    /// Registered provider name.
    pub provider: String,
    /// Required endpoint for self-hosted and cloud providers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub endpoint: Option<String>,
    /// Model or voice identifier.
    pub model: String,
    /// External secret name. Cloud providers require it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub credentials_ref: Option<String>,
    /// The user allowed this cloud provider to receive session data.
    #[serde(default)]
    pub consent: bool,
}

/// Named STT, thinker, and TTS configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AiConfiguration {
    /// Stored id.
    pub id: Uuid,
    /// User-facing name.
    pub name: String,
    /// Speech-to-text provider.
    pub stt: ProviderConfig,
    /// Language-model provider.
    pub thinker: ProviderConfig,
    /// Text-to-speech provider.
    pub tts: ProviderConfig,
    /// Creation time.
    pub created_at: DateTime<Utc>,
    /// Last replacement time.
    pub updated_at: DateTime<Utc>,
}

/// Body of `PUT /api/config/ai`.
#[derive(Debug, Clone, Deserialize)]
pub struct SaveAiConfiguration {
    /// Configuration id. Omitted values create a row.
    #[serde(default)]
    pub id: Option<Uuid>,
    /// User-facing name.
    pub name: String,
    /// Speech-to-text provider.
    pub stt: ProviderConfig,
    /// Language-model provider.
    pub thinker: ProviderConfig,
    /// Text-to-speech provider.
    pub tts: ProviderConfig,
}

/// Features a provider can expose to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCapabilities {
    /// The provider can emit incremental output.
    pub supports_streaming: bool,
    /// The provider can emit a partial transcript.
    pub supports_partial_transcript: bool,
    /// The provider can return word times.
    pub supports_word_timestamps: bool,
    /// An in-flight request can be cancelled.
    pub supports_cancellation: bool,
    /// Highest accepted sample rate in hertz, when audio applies.
    pub max_sample_rate: Option<u32>,
    /// Language codes the provider accepts.
    pub supported_languages: Vec<String>,
}

/// One provider the UI can select.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderDescriptor {
    /// Capability this descriptor belongs to.
    pub capability: Capability,
    /// Stable provider name stored in configuration.
    pub provider: String,
    /// Allowed deployment location.
    pub kind: ProviderKind,
    /// Features the UI may rely on.
    pub capabilities: ProviderCapabilities,
}

/// Result of checking one saved configuration slot.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProviderTestResult {
    /// Slot that was checked.
    pub capability: Capability,
    /// Registered provider name.
    pub provider: String,
    /// Whether the configuration matches the registry.
    pub ok: bool,
    /// Short result. A failure names the missing requirement.
    pub message: String,
}

/// Persistence port for AI configuration rows.
#[async_trait::async_trait]
pub trait AiConfigurationRepository: Send + Sync {
    /// Insert or replace a configuration.
    async fn save(&self, configuration: &AiConfiguration) -> Result<(), CoreError>;

    /// Load one configuration.
    async fn get(&self, id: Uuid) -> Result<Option<AiConfiguration>, CoreError>;

    /// List configurations, newest update first.
    async fn list(&self) -> Result<Vec<AiConfiguration>, CoreError>;
}

/// SQLite implementation of [`AiConfigurationRepository`].
#[derive(Clone)]
pub struct SqliteAiConfigurationRepository {
    pool: SqlitePool,
}

impl SqliteAiConfigurationRepository {
    /// Store rows in an already migrated pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl AiConfigurationRepository for SqliteAiConfigurationRepository {
    async fn save(&self, configuration: &AiConfiguration) -> Result<(), CoreError> {
        sqlx::query(
            "INSERT INTO ai_configurations
                (id, name, stt_config, thinker_config, tts_config, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                name = excluded.name,
                stt_config = excluded.stt_config,
                thinker_config = excluded.thinker_config,
                tts_config = excluded.tts_config,
                updated_at = excluded.updated_at",
        )
        .bind(configuration.id.to_string())
        .bind(&configuration.name)
        .bind(serde_json::to_string(&configuration.stt).expect("provider config serializes"))
        .bind(serde_json::to_string(&configuration.thinker).expect("provider config serializes"))
        .bind(serde_json::to_string(&configuration.tts).expect("provider config serializes"))
        .bind(configuration.created_at.to_rfc3339())
        .bind(configuration.updated_at.to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn get(&self, id: Uuid) -> Result<Option<AiConfiguration>, CoreError> {
        let row = sqlx::query(&format!(
            "SELECT {CONFIG_COLUMNS} FROM ai_configurations WHERE id = ?"
        ))
        .bind(id.to_string())
        .fetch_optional(&self.pool)
        .await?;
        row.map(|row| map_configuration(&row)).transpose()
    }

    async fn list(&self) -> Result<Vec<AiConfiguration>, CoreError> {
        let rows = sqlx::query(&format!(
            "SELECT {CONFIG_COLUMNS} FROM ai_configurations ORDER BY updated_at DESC, id DESC"
        ))
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(map_configuration).collect()
    }
}

/// Built-in providers and the configuration service.
#[derive(Clone)]
pub struct AiConfigurationService<R> {
    repository: Arc<R>,
    providers: Vec<ProviderDescriptor>,
}

impl<R> AiConfigurationService<R>
where
    R: AiConfigurationRepository,
{
    /// Build a service with the built-in registry.
    #[must_use]
    pub fn new(repository: R) -> Self {
        Self {
            repository: Arc::new(repository),
            providers: builtin_providers(),
        }
    }

    /// Providers the UI may select.
    #[must_use]
    pub fn providers(&self) -> &[ProviderDescriptor] {
        &self.providers
    }

    /// Return every saved configuration, newest first.
    ///
    /// # Errors
    ///
    /// Returns a database error when the query fails.
    pub async fn list(&self) -> Result<Vec<AiConfiguration>, CoreError> {
        self.repository.list().await
    }

    /// Validate and store a configuration.
    ///
    /// # Errors
    ///
    /// Returns a validation error when a slot does not match the registry,
    /// [`CoreError::NotFound`] when an update id is unknown, and a database
    /// error when the write fails.
    pub async fn save(&self, input: SaveAiConfiguration) -> Result<AiConfiguration, CoreError> {
        let name = input.name.trim().to_owned();
        if name.is_empty() {
            return Err(CoreError::EmptyName);
        }
        validate_slot(Capability::Stt, &input.stt, &self.providers)?;
        validate_slot(Capability::Thinker, &input.thinker, &self.providers)?;
        validate_slot(Capability::Tts, &input.tts, &self.providers)?;

        let now = Utc::now();
        let configuration = if let Some(id) = input.id {
            let existing = self.repository.get(id).await?.ok_or(CoreError::NotFound)?;
            AiConfiguration {
                id,
                name,
                stt: normalize(input.stt),
                thinker: normalize(input.thinker),
                tts: normalize(input.tts),
                created_at: existing.created_at,
                updated_at: now,
            }
        } else {
            AiConfiguration {
                id: Uuid::now_v7(),
                name,
                stt: normalize(input.stt),
                thinker: normalize(input.thinker),
                tts: normalize(input.tts),
                created_at: now,
                updated_at: now,
            }
        };
        self.repository.save(&configuration).await?;
        Ok(configuration)
    }

    /// Check that one saved slot still matches the registry.
    ///
    /// This does not contact a remote provider.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::NotFound`] when the configuration id is unknown.
    pub async fn test(
        &self,
        configuration_id: Uuid,
        capability: Capability,
    ) -> Result<ProviderTestResult, CoreError> {
        let configuration = self
            .repository
            .get(configuration_id)
            .await?
            .ok_or(CoreError::NotFound)?;
        let config = match capability {
            Capability::Stt => &configuration.stt,
            Capability::Thinker => &configuration.thinker,
            Capability::Tts => &configuration.tts,
        };
        match validate_slot(capability, config, &self.providers) {
            Ok(()) => Ok(ProviderTestResult {
                capability,
                provider: config.provider.clone(),
                ok: true,
                message: format!(
                    "{} provider '{}' is configured",
                    capability.as_str(),
                    config.provider
                ),
            }),
            Err(error) => Ok(ProviderTestResult {
                capability,
                provider: config.provider.clone(),
                ok: false,
                message: error.to_string(),
            }),
        }
    }
}

/// One model a provider offers, and whether it can run without a download.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelOption {
    /// Identifier sent to the provider.
    pub id: String,
    /// What the settings list shows.
    pub label: String,
    /// Size on disk for a local model, in megabytes.
    pub size_mb: Option<u32>,
    /// How fast a local model runs, from "very-fast" to "slow".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub speed: Option<String>,
    /// How accurate a local model is, from "decent" to "very-high".
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accuracy: Option<String>,
    /// The model transcribes or speaks English only.
    pub english_only: bool,
    /// The file has to be downloaded before the model can run.
    pub local: bool,
}

/// The models one provider offers, and which one is selected when a session
/// does not pick.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ModelCatalog {
    /// Capability the catalog belongs to.
    pub capability: Capability,
    /// Provider name.
    pub provider: String,
    /// Model used when none is chosen.
    pub default_model: String,
    /// Every model the provider can run.
    pub models: Vec<ModelOption>,
}

/// One catalog entry, when the provider lists that model.
#[must_use]
pub fn catalog_model(capability: &Capability, provider: &str, model: &str) -> Option<ModelOption> {
    model_catalogs()
        .into_iter()
        .find(|catalog| catalog.capability == *capability && catalog.provider == provider)?
        .models
        .into_iter()
        .find(|option| option.id == model)
}

/// The catalogs the settings pages and the session model picker share.
#[must_use]
pub fn model_catalogs() -> Vec<ModelCatalog> {
    MODEL_CATALOGS
        .iter()
        .map(|catalog| ModelCatalog {
            capability: catalog.0,
            provider: catalog.1.to_owned(),
            default_model: catalog.2.to_owned(),
            models: catalog.3.iter().map(Into::into).collect(),
        })
        .collect()
}

/// Models advertised by an OpenAI-compatible gateway on this machine.
///
/// The endpoint must be `http` or `https` and its host must be loopback
/// (`127.0.0.1`, `localhost`, or `::1`). Anything else is rejected so the
/// server cannot be turned into a proxy to an arbitrary address. A bearer
/// token is sent only when `key` is non-empty. At most 200 ids are returned.
///
/// # Errors
///
/// Returns [`CoreError::InvalidConfiguration`] for a bad URL or a non-loopback
/// host, and the same error when the request or the JSON body fails.
pub async fn discover_models(
    endpoint: &str,
    key: Option<&str>,
) -> Result<Vec<ModelOption>, CoreError> {
    let base = checked_endpoint(endpoint)?;
    fetch_models(&base, key).await
}

/// Reject a discovery endpoint that is not an http(s) loopback URL.
///
/// # Errors
///
/// Returns [`CoreError::InvalidConfiguration`] when the scheme or host is not allowed.
pub fn checked_endpoint(endpoint: &str) -> Result<String, CoreError> {
    loopback_endpoint(endpoint)
}

/// Contact a gateway already known to be loopback.
async fn fetch_models(base: &str, key: Option<&str>) -> Result<Vec<ModelOption>, CoreError> {
    let url = format!("{}/models", base.trim_end_matches('/'));
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(5))
        .build()
        .map_err(|error| CoreError::InvalidConfiguration(error.to_string()))?;
    let mut request = client.get(url);
    if let Some(key) = key.map(str::trim).filter(|value| !value.is_empty()) {
        request = request.bearer_auth(key);
    }
    let body: serde_json::Value = request
        .send()
        .await
        .map_err(|error| CoreError::InvalidConfiguration(error.to_string()))?
        .json()
        .await
        .map_err(|error| CoreError::InvalidConfiguration(error.to_string()))?;
    let models = body
        .get("data")
        .and_then(|data| data.as_array())
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("id").and_then(|id| id.as_str()))
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .take(200)
                .map(|id| ModelOption {
                    id: id.to_owned(),
                    label: id.to_owned(),
                    size_mb: None,
                    speed: None,
                    accuracy: None,
                    english_only: false,
                    local: false,
                })
                .collect()
        })
        .unwrap_or_default();
    Ok(models)
}

/// Accept only an http(s) URL whose host is loopback.
fn loopback_endpoint(endpoint: &str) -> Result<String, CoreError> {
    // Loopback only: the server must not become a proxy to arbitrary hosts.
    let endpoint = endpoint.trim();
    let url = reqwest::Url::parse(endpoint).map_err(|error| {
        CoreError::InvalidConfiguration(format!("endpoint is not a URL: {error}"))
    })?;
    if url.scheme() != "http" && url.scheme() != "https" {
        return Err(CoreError::InvalidConfiguration(
            "endpoint must start with http:// or https://".to_owned(),
        ));
    }
    let host = url
        .host_str()
        .ok_or_else(|| CoreError::InvalidConfiguration("endpoint host is missing".to_owned()))?;
    if host != "127.0.0.1" && host != "localhost" && host != "::1" {
        return Err(CoreError::InvalidConfiguration(
            "model discovery only contacts loopback hosts".to_owned(),
        ));
    }
    Ok(endpoint.trim_end_matches('/').to_owned())
}

/// Whether a model belongs to a provider's catalog.
#[must_use]
pub fn catalog_has(capability: Capability, provider: &str, model: &str) -> bool {
    MODEL_CATALOGS.iter().any(|catalog| {
        catalog.0 == capability
            && catalog.1 == provider
            && catalog.3.iter().any(|option| option.0 == model)
    })
}

/// One file a local model needs, and where it is published.
///
/// A model can need several files. `file_name` is a single name; the slash
/// inside an ONNX URL path never reaches the disk name.
#[derive(Debug, Clone, Copy)]
pub struct LocalModelFile {
    /// Hugging Face URL.
    pub url: &'static str,
    /// File name written on disk, never a path.
    pub file_name: &'static str,
    /// Expected size, in megabytes, for the progress display.
    pub size_mb: u32,
}

/// A local model Talkcer can download itself, and every file it needs.
#[derive(Debug, Clone, Copy)]
pub struct LocalModel {
    /// Capability.
    pub capability: Capability,
    /// Provider name.
    pub provider: &'static str,
    /// Model identifier.
    pub model: &'static str,
    /// The files the model cannot run without.
    pub files: &'static [LocalModelFile],
}

/// One downloadable model, kept in a `const` so the two files it needs can live
/// in the same place as the metadata.
struct Downloadable(
    Capability,
    &'static str,
    &'static str,
    &'static str,
    &'static str,
    u32,
    u32,
);

/// Every local model Talkcer can download itself.
///
/// Built once. The model list never changes while the process runs.
#[must_use]
pub fn local_models() -> &'static [LocalModel] {
    static MODELS: std::sync::OnceLock<Vec<LocalModel>> = std::sync::OnceLock::new();
    MODELS.get_or_init(local_models_list)
}

/// One local model, when Talkcer knows how to download it.
#[must_use]
pub fn local_model(capability: Capability, provider: &str, model: &str) -> Option<LocalModel> {
    local_models().iter().copied().find(|entry| {
        entry.capability == capability && entry.provider == provider && entry.model == model
    })
}

struct StaticModel(
    &'static str,
    &'static str,
    Option<u32>,
    bool,
    bool,
    Option<&'static str>,
    Option<&'static str>,
);

impl From<&StaticModel> for ModelOption {
    fn from(model: &StaticModel) -> Self {
        Self {
            id: model.0.to_owned(),
            label: model.1.to_owned(),
            size_mb: model.2,
            english_only: model.3,
            local: model.4,
            speed: model.5.map(str::to_owned),
            accuracy: model.6.map(str::to_owned),
        }
    }
}

type Catalog = (
    Capability,
    &'static str,
    &'static str,
    &'static [StaticModel],
);

const MODEL_CATALOGS: &[Catalog] = &[
    (
        Capability::Thinker,
        "ollama",
        "llama3.2",
        &[
            StaticModel("llama3.2", "Llama 3.2", Some(2_000), false, true, None, None),
            StaticModel("qwen2.5", "Qwen 2.5", Some(4_700), false, true, None, None),
        ],
    ),
    (
        Capability::Thinker,
        "openai",
        "gpt-4o-mini",
        &[
            StaticModel("gpt-4o-mini", "GPT-4o mini", None, false, false, None, None),
            StaticModel("gpt-4o", "GPT-4o", None, false, false, None, None),
        ],
    ),
    (
        Capability::Thinker,
        "groq",
        "llama-3.3-70b-versatile",
        &[StaticModel(
            "llama-3.3-70b-versatile",
            "Llama 3.3 70B",
            None,
            false,
            false,
            None,
            None,
        )],
    ),
    (
        Capability::Thinker,
        "deepseek",
        "deepseek-chat",
        &[StaticModel(
            "deepseek-chat",
            "DeepSeek Chat",
            None,
            false,
            false,
            None,
            None,
        )],
    ),
    (
        Capability::Stt,
        "whisper",
        "tiny",
        &[
            StaticModel("moonshine-tiny", "Moonshine Tiny", Some(26), true, true, Some("very-fast"), Some("good")),
            StaticModel("moonshine-base", "Moonshine Base", Some(60), true, true, Some("very-fast"), Some("very-high")),
            StaticModel("parakeet-tdt-0.6b-v3", "Parakeet TDT 0.6B v3", Some(670), false, true, Some("very-fast"), Some("very-high")),
            StaticModel("nemotron-3.5-asr-streaming", "Nemotron 3.5 ASR Streaming", Some(793), false, true, Some("fast"), Some("high")),
            StaticModel("distil-small.en", "Distil Small EN", Some(164), true, true, Some("very-fast"), Some("high")),
            StaticModel("distil-medium.en", "Distil Medium EN", Some(383), true, true, Some("fast"), Some("very-high")),
            StaticModel("distil-large-v3", "Distil Large v3", Some(731), true, true, Some("medium"), Some("very-high")),
            StaticModel("whisper-large-v3-turbo", "Whisper Large v3 Turbo", Some(1_031), false, true, Some("medium"), Some("very-high")),
            StaticModel("tiny.en", "Whisper Tiny English", Some(39), true, true, Some("very-fast"), Some("decent")),
            StaticModel("tiny", "Whisper Tiny", Some(74), false, true, Some("very-fast"), Some("decent")),
            StaticModel("base.en", "Whisper Base English", Some(142), true, true, Some("fast"), Some("good")),
            StaticModel("base", "Whisper Base", Some(145), false, true, Some("fast"), Some("good")),
            StaticModel("small.en", "Whisper Small English", Some(244), true, true, Some("medium"), Some("high")),
            StaticModel("small", "Whisper Small", Some(466), false, true, Some("medium"), Some("high")),
            StaticModel("medium.en", "Whisper Medium English", Some(1_500), true, true, Some("slow"), Some("very-high")),
            StaticModel("medium", "Whisper Medium", Some(1_530), false, true, Some("slow"), Some("very-high")),
        ],
    ),
    (
        Capability::Stt,
        "deepgram",
        "nova-3",
        &[
            StaticModel("nova-3", "Nova-3", None, false, false, None, None),
            StaticModel("nova-3-medical", "Nova-3 Medical", None, true, false, None, None),
            StaticModel("nova-2", "Nova-2", None, false, false, None, None),
        ],
    ),
    (
        Capability::Stt,
        "stt-openai",
        "gpt-4o-mini-transcribe",
        &[
            StaticModel(
                "gpt-4o-mini-transcribe",
                "GPT-4o mini Transcribe",
                None,
                false,
                false,
                None,
                None,
            ),
            StaticModel("gpt-4o-transcribe", "GPT-4o Transcribe", None, false, false, None, None),
        ],
    ),
    (
        Capability::Tts,
        "piper",
        "en_US-lessac-medium",
        &[StaticModel(
            "en_US-lessac-medium",
            "Piper Lessac (English)",
            Some(63),
            true,
            true,
            Some("fast"),
            Some("high"),
        )],
    ),
    (
        Capability::Tts,
        "tts-openai",
        "gpt-4o-mini-tts",
        &[StaticModel(
            "gpt-4o-mini-tts",
            "GPT-4o mini TTS",
            None,
            false,
            false,
            None,
            None,
        )],
    ),
];

/// Builds the encoder and merged-decoder pair every Whisper-family model shares.
///
/// `variant` is the filename suffix, `int8` for Moonshine and `quantized` for
/// the Xenova and Distil-Whisper exports. Sizes were read from the repositories.
fn pair(entry: &Downloadable) -> LocalModel {
    let files: &'static [LocalModelFile] = Box::leak(Box::new([
        file(entry, &format!("encoder_model_{}.onnx", entry.4), entry.5),
        file(entry, &format!("decoder_model_merged_{}.onnx", entry.4), entry.6),
    ]));
    LocalModel {
        capability: entry.0,
        provider: entry.1,
        model: entry.2,
        files,
    }
}

fn file(entry: &Downloadable, name: &str, size_mb: u32) -> LocalModelFile {
    LocalModelFile {
        url: Box::leak(
            format!("https://huggingface.co/{}/resolve/main/onnx/{name}", entry.3).into_boxed_str(),
        ),
        file_name: Box::leak(format!("{}-{name}", entry.2).into_boxed_str()),
        size_mb,
    }
}

const DOWNLOADABLE: &[Downloadable] = &[
    Downloadable(Capability::Stt, "whisper", "moonshine-tiny", "onnx-community/moonshine-tiny-ONNX", "int8", 8, 19),
    Downloadable(Capability::Stt, "whisper", "moonshine-base", "onnx-community/moonshine-base-ONNX", "int8", 20, 40),
    Downloadable(Capability::Stt, "whisper", "distil-small.en", "distil-whisper/distil-small.en", "quantized", 88, 76),
    Downloadable(Capability::Stt, "whisper", "distil-medium.en", "distil-whisper/distil-medium.en", "quantized", 299, 85),
    Downloadable(Capability::Stt, "whisper", "distil-large-v3", "distil-whisper/distil-large-v3", "quantized", 615, 116),
    Downloadable(Capability::Stt, "whisper", "distil-large-v2", "distil-whisper/distil-large-v2", "quantized", 615, 116),
    Downloadable(Capability::Stt, "whisper", "whisper-large-v3-turbo", "onnx-community/whisper-large-v3-turbo", "quantized", 615, 420),
    Downloadable(Capability::Stt, "whisper", "tiny.en", "Xenova/whisper-tiny.en", "quantized", 10, 29),
    Downloadable(Capability::Stt, "whisper", "tiny", "Xenova/whisper-tiny", "quantized", 10, 29),
    Downloadable(Capability::Stt, "whisper", "base.en", "Xenova/whisper-base.en", "quantized", 22, 51),
    Downloadable(Capability::Stt, "whisper", "base", "Xenova/whisper-base", "quantized", 22, 51),
    Downloadable(Capability::Stt, "whisper", "small.en", "Xenova/whisper-small.en", "quantized", 88, 150),
    Downloadable(Capability::Stt, "whisper", "small", "Xenova/whisper-small", "quantized", 88, 150),
    Downloadable(Capability::Stt, "whisper", "medium.en", "Xenova/whisper-medium.en", "quantized", 299, 441),
    Downloadable(Capability::Stt, "whisper", "medium", "Xenova/whisper-medium", "quantized", 299, 441),
];

fn local_models_list() -> Vec<LocalModel> {
    let mut models: Vec<LocalModel> = DOWNLOADABLE.iter().map(pair).collect();
    models.push(LocalModel {
        capability: Capability::Tts,
        provider: "piper",
        model: "en_US-lessac-medium",
        files: &[LocalModelFile {
            url: "https://huggingface.co/rhasspy/piper-voices/resolve/main/en/en_US/lessac/medium/en_US-lessac-medium.onnx",
            file_name: "en_US-lessac-medium.onnx",
            size_mb: 63,
        }],
    });
    models
}

/// Kind registered for one provider, when the name is in the built-in list.
///
/// Callers such as thinker selection use this so they do not keep a second copy
/// of the registry. An unknown name returns `None`.
#[must_use]
pub fn provider_kind(capability: Capability, provider: &str) -> Option<ProviderKind> {
    builtin_providers()
        .into_iter()
        .find(|item| item.capability == capability && item.provider == provider)
        .map(|item| item.kind)
}

fn builtin_providers() -> Vec<ProviderDescriptor> {
    vec![
        descriptor(
            Capability::Thinker,
            "ollama",
            ProviderKind::SelfHosted,
            true,
            false,
            false,
            true,
            None,
            &["en", "id"],
        ),
        descriptor(
            Capability::Thinker,
            "openai",
            ProviderKind::Cloud,
            true,
            false,
            false,
            true,
            None,
            &["en", "id"],
        ),
        descriptor(
            Capability::Thinker,
            "groq",
            ProviderKind::Cloud,
            true,
            false,
            false,
            true,
            None,
            &["en", "id"],
        ),
        descriptor(
            Capability::Thinker,
            "deepseek",
            ProviderKind::Cloud,
            true,
            false,
            false,
            true,
            None,
            &["en", "id"],
        ),
        descriptor(
            Capability::Thinker,
            "thinker-custom",
            ProviderKind::SelfHosted,
            true,
            false,
            false,
            true,
            None,
            &["en", "id"],
        ),
        // Gateways expose a live model list, so they have no catalog entry.
        descriptor(
            Capability::Thinker,
            "9router",
            ProviderKind::SelfHosted,
            true,
            false,
            false,
            true,
            None,
            &["en", "id"],
        ),
        descriptor(
            Capability::Thinker,
            "litellm",
            ProviderKind::SelfHosted,
            true,
            false,
            false,
            true,
            None,
            &["en", "id"],
        ),
        descriptor(
            Capability::Stt,
            "whisper",
            ProviderKind::Embedded,
            true,
            true,
            true,
            true,
            Some(16_000),
            &["en", "id"],
        ),
        descriptor(
            Capability::Stt,
            "deepgram",
            ProviderKind::Cloud,
            true,
            true,
            true,
            true,
            Some(48_000),
            &["en", "id"],
        ),
        descriptor(
            Capability::Stt,
            "stt-openai",
            ProviderKind::Cloud,
            true,
            true,
            false,
            true,
            Some(48_000),
            &["en", "id"],
        ),
        descriptor(
            Capability::Tts,
            "piper",
            ProviderKind::SelfHosted,
            false,
            false,
            false,
            true,
            Some(22_050),
            &["en"],
        ),
        descriptor(
            Capability::Tts,
            "tts-openai",
            ProviderKind::Cloud,
            false,
            false,
            false,
            true,
            Some(24_000),
            &["en", "id"],
        ),
        descriptor(
            Capability::Tts,
            "tts-custom",
            ProviderKind::SelfHosted,
            false,
            false,
            false,
            true,
            Some(24_000),
            &["en", "id"],
        ),
        descriptor(
            Capability::Stt,
            "embedded-stt",
            ProviderKind::Embedded,
            true,
            true,
            false,
            true,
            Some(16_000),
            &["en", "id"],
        ),
        descriptor(
            Capability::Stt,
            "cloud-stt",
            ProviderKind::Cloud,
            true,
            true,
            true,
            true,
            Some(48_000),
            &["en", "id"],
        ),
        descriptor(
            Capability::Stt,
            "self-hosted-stt",
            ProviderKind::SelfHosted,
            true,
            false,
            false,
            true,
            Some(48_000),
            &["en", "id"],
        ),
        descriptor(
            Capability::Thinker,
            "embedded-thinker",
            ProviderKind::Embedded,
            false,
            false,
            false,
            true,
            None,
            &["en", "id"],
        ),
        descriptor(
            Capability::Thinker,
            "cloud-thinker",
            ProviderKind::Cloud,
            true,
            false,
            false,
            true,
            None,
            &["en", "id"],
        ),
        descriptor(
            Capability::Thinker,
            "self-hosted-thinker",
            ProviderKind::SelfHosted,
            true,
            false,
            false,
            true,
            None,
            &["en", "id"],
        ),
        descriptor(
            Capability::Tts,
            "embedded-tts",
            ProviderKind::Embedded,
            false,
            false,
            false,
            true,
            Some(16_000),
            &["en", "id"],
        ),
        descriptor(
            Capability::Tts,
            "cloud-tts",
            ProviderKind::Cloud,
            false,
            false,
            false,
            true,
            Some(24_000),
            &["en", "id"],
        ),
        descriptor(
            Capability::Tts,
            "self-hosted-tts",
            ProviderKind::SelfHosted,
            false,
            false,
            false,
            true,
            Some(24_000),
            &["en", "id"],
        ),
        descriptor(
            Capability::Tts,
            "disabled-tts",
            ProviderKind::Embedded,
            false,
            false,
            false,
            false,
            None,
            &[],
        ),
    ]
}

fn descriptor(
    capability: Capability,
    provider: &str,
    kind: ProviderKind,
    streaming: bool,
    partial: bool,
    timestamps: bool,
    cancellation: bool,
    sample_rate: Option<u32>,
    languages: &[&str],
) -> ProviderDescriptor {
    ProviderDescriptor {
        capability,
        provider: provider.to_owned(),
        kind,
        capabilities: ProviderCapabilities {
            supports_streaming: streaming,
            supports_partial_transcript: partial,
            supports_word_timestamps: timestamps,
            supports_cancellation: cancellation,
            max_sample_rate: sample_rate,
            supported_languages: languages
                .iter()
                .map(|language| (*language).to_owned())
                .collect(),
        },
    }
}

fn validate_slot(
    capability: Capability,
    config: &ProviderConfig,
    providers: &[ProviderDescriptor],
) -> Result<(), CoreError> {
    reject_secret(&config.provider)?;
    reject_secret(config.endpoint.as_deref().unwrap_or(""))?;
    reject_secret(&config.model)?;
    reject_secret(config.credentials_ref.as_deref().unwrap_or(""))?;

    let Some(descriptor) = providers
        .iter()
        .find(|item| item.capability == capability && item.provider == config.provider)
    else {
        return Err(CoreError::UnknownProvider(config.provider.clone()));
    };
    if descriptor.kind != config.kind {
        return Err(CoreError::InvalidConfiguration(format!(
            "{} provider '{}' requires kind {}",
            capability.as_str(),
            config.provider,
            descriptor.kind.as_str()
        )));
    }
    if config.model.trim().is_empty() {
        return Err(CoreError::InvalidConfiguration(format!(
            "{} model must not be empty",
            capability.as_str()
        )));
    }
    let on_device = config.kind == ProviderKind::Embedded
        && local_model(capability, &config.provider, config.model.trim()).is_some();
    if matches!(config.kind, ProviderKind::SelfHosted | ProviderKind::Cloud)
        && !on_device
        && config.endpoint.as_deref().unwrap_or("").trim().is_empty()
    {
        return Err(CoreError::InvalidConfiguration(format!(
            "{} endpoint is required for {}",
            capability.as_str(),
            config.kind.as_str()
        )));
    }
    if config.kind == ProviderKind::Cloud
        && config
            .credentials_ref
            .as_deref()
            .unwrap_or("")
            .trim()
            .is_empty()
    {
        return Err(CoreError::InvalidConfiguration(format!(
            "{} credentials_ref is required for cloud",
            capability.as_str()
        )));
    }
    if config.kind == ProviderKind::Cloud && !config.consent {
        return Err(CoreError::InvalidConfiguration(format!(
            "{} cloud provider needs consent before session data leaves this computer",
            capability.as_str()
        )));
    }
    // A custom OpenAI-compatible provider names its own model, and the
    // original generic providers predate the catalogs, so only a catalogued
    // provider is constrained to its list.
    let catalogued = MODEL_CATALOGS
        .iter()
        .any(|catalog| catalog.0 == capability && catalog.1 == config.provider);
    if catalogued && !catalog_has(capability, &config.provider, config.model.trim()) {
        return Err(CoreError::InvalidConfiguration(format!(
            "{} model '{}' is not offered by {}",
            capability.as_str(),
            config.model.trim(),
            config.provider
        )));
    }
    if let Some(endpoint) = &config.endpoint {
        let endpoint = endpoint.trim();
        if !endpoint.is_empty()
            && !endpoint.starts_with("http://")
            && !endpoint.starts_with("https://")
        {
            return Err(CoreError::InvalidConfiguration(format!(
                "{} endpoint must start with http:// or https://",
                capability.as_str()
            )));
        }
    }
    Ok(())
}

/// Stop a provider call that runs longer than `limit`.
///
/// # Errors
///
/// Returns [`CoreError::ProviderTimeout`] when the limit is reached, or the inner error.
pub async fn run_with_timeout<T>(
    limit: std::time::Duration,
    work: impl std::future::Future<Output = Result<T, CoreError>>,
) -> Result<T, CoreError> {
    match tokio::time::timeout(limit, work).await {
        Ok(result) => result,
        Err(_) => Err(CoreError::ProviderTimeout),
    }
}

fn reject_secret(value: &str) -> Result<(), CoreError> {
    let lower = value.to_ascii_lowercase();
    if lower.contains("api_key") || lower.contains("secret") || lower.contains("bearer ") {
        return Err(CoreError::InvalidConfiguration(
            "store a credentials_ref name, not a secret value".to_owned(),
        ));
    }
    Ok(())
}

fn normalize(mut config: ProviderConfig) -> ProviderConfig {
    config.provider = config.provider.trim().to_owned();
    config.model = config.model.trim().to_owned();
    config.endpoint = config
        .endpoint
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    config.credentials_ref = config
        .credentials_ref
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty());
    config
}

fn map_configuration(row: &sqlx::sqlite::SqliteRow) -> Result<AiConfiguration, CoreError> {
    let id: String = row.try_get("id")?;
    let stt: String = row.try_get("stt_config")?;
    let thinker: String = row.try_get("thinker_config")?;
    let tts: String = row.try_get("tts_config")?;
    let created_at: String = row.try_get("created_at")?;
    let updated_at: String = row.try_get("updated_at")?;
    Ok(AiConfiguration {
        id: parse_uuid(&id)?,
        name: row.try_get("name")?,
        stt: parse_json(&stt)?,
        thinker: parse_json(&thinker)?,
        tts: parse_json(&tts)?,
        created_at: parse_time(&created_at)?,
        updated_at: parse_time(&updated_at)?,
    })
}

fn parse_json(value: &str) -> Result<ProviderConfig, CoreError> {
    serde_json::from_str(value).map_err(|error| {
        CoreError::InvalidConfiguration(format!("stored configuration is invalid: {error}"))
    })
}

fn parse_uuid(value: &str) -> Result<Uuid, CoreError> {
    Uuid::parse_str(value)
        .map_err(|error| CoreError::Database(sqlx::Error::Decode(Box::new(error))))
}

fn parse_time(value: &str) -> Result<DateTime<Utc>, CoreError> {
    DateTime::parse_from_rfc3339(value)
        .map(|time| time.with_timezone(&Utc))
        .map_err(|error| CoreError::Database(sqlx::Error::Decode(Box::new(error))))
}
