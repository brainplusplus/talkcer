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
            let existing = self
                .repository
                .get(id)
                .await?
                .ok_or(CoreError::NotFound)?;
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

fn builtin_providers() -> Vec<ProviderDescriptor> {
    vec![
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
            supported_languages: languages.iter().map(|language| (*language).to_owned()).collect(),
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
    if matches!(config.kind, ProviderKind::SelfHosted | ProviderKind::Cloud)
        && config.endpoint.as_deref().unwrap_or("").trim().is_empty()
    {
        return Err(CoreError::InvalidConfiguration(format!(
            "{} endpoint is required for {}",
            capability.as_str(),
            config.kind.as_str()
        )));
    }
    if config.kind == ProviderKind::Cloud
        && config.credentials_ref.as_deref().unwrap_or("").trim().is_empty()
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
    serde_json::from_str(value)
        .map_err(|error| CoreError::InvalidConfiguration(format!("stored configuration is invalid: {error}")))
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
