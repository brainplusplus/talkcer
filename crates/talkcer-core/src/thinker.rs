//! Language-model (thinker) selection and adapters.
//!
//! A session points at a thinker the same way it points at an STT provider:
//! a saved configuration id, an inline `session.configuration.thinker` object,
//! or the embedded offline default. [`EmbeddedThinker`] keeps the deterministic
//! local templates so a session works with no network; [`HttpThinker`] calls an
//! OpenAI-compatible `/chat/completions` endpoint.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::json;

use crate::ai::{AiConfiguration, ProviderConfig, ProviderKind};
use crate::domain::Session;
use crate::error::CoreError;
use crate::timeline::AssistantAction;

/// The job a thinker is asked to do.
///
/// The embedded adapter maps each task to its deterministic local text, so the
/// offline behaviour is identical to the earlier stubs. A remote adapter renders
/// the same task as a chat prompt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ThinkerTask {
    /// A live-interview assistant action on one transcript.
    AssistantAction {
        /// Requested action.
        action: AssistantAction,
        /// Transcript the action targets.
        target: String,
        /// Output language, when the caller asked for one.
        language: Option<String>,
    },
    /// First mock-interview question.
    MockQuestion {
        /// Interview role from the session context.
        role: String,
    },
    /// Mock-interview follow-up.
    MockFollowUp {
        /// Interview role from the session context.
        role: String,
    },
    /// Mock-interview closing feedback.
    MockFeedback,
    /// Mock-interview summary line.
    MockSummary,
    /// Learning introduction.
    LearningIntro {
        /// Topic from the session context.
        topic: String,
    },
    /// Learning practice prompt shown at the start.
    LearningStartPractice {
        /// Topic from the session context.
        topic: String,
    },
    /// Learning discussion answer.
    LearningAnswer {
        /// Topic from the session context.
        topic: String,
        /// Learner question.
        question: String,
    },
    /// Learning practice prompt shown after a discussion.
    LearningPractice {
        /// Topic from the session context.
        topic: String,
    },
    /// Learning practice feedback.
    LearningFeedback {
        /// Topic from the session context.
        topic: String,
    },
    /// A question about whatever the current screen shows.
    ///
    /// The prompt already names the mode and carries the vision model's reading
    /// of the screen, so the thinker answers from that text.
    ScreenContext {
        /// The full prompt, mode stance included.
        prompt: String,
    },
    /// A typed chat message, optionally with profile context already prepended.
    Chat {
        /// User prompt, profile text included when the caller asked for it.
        prompt: String,
    },
}

impl ThinkerTask {
    /// A stable label stored in metadata and used in prompts.
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::AssistantAction { .. } => "assistant_action",
            Self::MockQuestion { .. } => "mock_question",
            Self::MockFollowUp { .. } => "mock_follow_up",
            Self::MockFeedback => "mock_feedback",
            Self::MockSummary => "mock_summary",
            Self::LearningIntro { .. } => "learning_intro",
            Self::LearningStartPractice { .. } => "learning_start_practice",
            Self::LearningAnswer { .. } => "learning_answer",
            Self::LearningPractice { .. } => "learning_practice",
            Self::LearningFeedback { .. } => "learning_feedback",
            Self::ScreenContext { .. } => "screen_context",
            Self::Chat { .. } => "chat",
        }
    }

    /// System instruction for a remote model.
    #[must_use]
    pub fn system(&self) -> String {
        match self {
            Self::AssistantAction { .. } => {
                "You are a real-time interview copilot. Answer in at most three sentences.".to_owned()
            }
            Self::MockQuestion { .. } | Self::MockFollowUp { .. } | Self::MockFeedback | Self::MockSummary => {
                "You are an interview coach running a mock interview. Ask one clear question at a time.".to_owned()
            }
            Self::LearningIntro { .. }
            | Self::LearningStartPractice { .. }
            | Self::LearningAnswer { .. }
            | Self::LearningPractice { .. }
            | Self::LearningFeedback { .. } => {
                "You are a patient tutor. Explain briefly and give one concrete example.".to_owned()
            }
            Self::ScreenContext { .. } => {
                "You are answering from what is currently on screen. Stay with the session mode and do not invent details the screen does not show.".to_owned()
            }
            Self::Chat { .. } => {
                "You are a real-time interview copilot. Answer in at most three sentences.".to_owned()
            }
        }
    }

    /// User prompt for a remote model.
    #[must_use]
    pub fn prompt(&self) -> String {
        match self {
            Self::AssistantAction {
                action,
                target,
                language,
            } => {
                let language = language
                    .as_deref()
                    .map(|value| format!(" Answer in {value}."))
                    .unwrap_or_default();
                format!(
                    "Action: {}. Transcript: \"{target}\".{language}",
                    action.as_str()
                )
            }
            Self::MockQuestion { role } => {
                format!("Ask an opening question for a {role} interview.")
            }
            Self::MockFollowUp { role } => {
                format!("Ask one follow-up question for a {role} interview.")
            }
            Self::MockFeedback => "Give one sentence of feedback on the answer.".to_owned(),
            Self::MockSummary => "Summarize the mock interview in one sentence.".to_owned(),
            Self::LearningIntro { topic } => {
                format!("Introduce the topic {topic} in one sentence.")
            }
            Self::LearningStartPractice { topic } => {
                format!("Give one practice task for {topic}.")
            }
            Self::LearningAnswer { topic, question } => {
                format!("Answer this question about {topic}: {question}")
            }
            Self::LearningPractice { topic } => {
                format!("Give one practice task about a tradeoff in {topic}.")
            }
            Self::LearningFeedback { topic } => {
                format!("Give one sentence of feedback on an answer about {topic}.")
            }
            Self::ScreenContext { prompt } | Self::Chat { prompt } => prompt.clone(),
        }
    }
}

/// The text a thinker produced, plus the adapter that produced it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ThinkerReply {
    /// Generated text.
    pub text: String,
    /// Registry name of the adapter.
    pub provider: String,
}

/// Generates one text reply for a task.
#[async_trait::async_trait]
pub trait Thinker: Send + Sync + 'static {
    /// Produce a reply.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] when the backend fails.
    async fn complete(&self, task: &ThinkerTask) -> Result<ThinkerReply, CoreError>;

    /// Registry name of the adapter.
    fn provider(&self) -> &'static str;
}

/// Deterministic offline thinker that mirrors the original local templates.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmbeddedThinker;

#[async_trait::async_trait]
impl Thinker for EmbeddedThinker {
    async fn complete(&self, task: &ThinkerTask) -> Result<ThinkerReply, CoreError> {
        let text = match task {
            ThinkerTask::AssistantAction { action, target, .. } => {
                format!(
                    "Stub response for action \"{}\" targeting utterance {target}.",
                    action.as_str()
                )
            }
            ThinkerTask::MockQuestion { role } => {
                format!("Tell me about your experience as a {role}.")
            }
            ThinkerTask::MockFollowUp { role } => {
                format!("What tradeoff would you make in that {role} example?")
            }
            ThinkerTask::MockFeedback => {
                "Your answers stay on the requested topic and include a concrete example."
                    .to_owned()
            }
            ThinkerTask::MockSummary => {
                "Summary: the interview covered experience and one tradeoff.".to_owned()
            }
            ThinkerTask::LearningIntro { topic } => {
                format!("Let's study {topic}. Ask a question when you are ready.")
            }
            ThinkerTask::LearningStartPractice { topic } => {
                format!("Practice: explain one way to improve {topic}.")
            }
            ThinkerTask::LearningAnswer { topic, question } => {
                format!("For {topic}, start from the question: {question}")
            }
            ThinkerTask::LearningPractice { topic } => {
                format!("Practice: name one tradeoff in {topic}.")
            }
            ThinkerTask::LearningFeedback { topic } => {
                format!("Feedback for {topic}: your answer is specific enough to continue.")
            }
            ThinkerTask::ScreenContext { prompt } => format!("Stub screen answer: {prompt}"),
            ThinkerTask::Chat { prompt } => {
                let excerpt: String = prompt.chars().take(80).collect();
                format!("(embedded) {excerpt}")
            }
        };
        Ok(ThinkerReply {
            text,
            provider: self.provider().to_owned(),
        })
    }

    fn provider(&self) -> &'static str {
        "embedded-thinker"
    }
}

/// Thinker over an OpenAI-compatible chat endpoint.
#[derive(Debug, Clone)]
pub struct HttpThinker {
    endpoint: String,
    model: String,
    language: String,
    credential: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ChatResponse {
    choices: Vec<ChatChoice>,
}

#[derive(Debug, Deserialize)]
struct ChatChoice {
    message: ChatMessage,
}

#[derive(Debug, Deserialize)]
struct ChatMessage {
    content: String,
}

impl HttpThinker {
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

    /// The URL a completion request is posted to.
    #[must_use]
    pub fn request_url(&self) -> String {
        let base = self.endpoint.trim_end_matches('/');
        if base.ends_with("/chat/completions") {
            base.to_owned()
        } else {
            format!("{base}/chat/completions")
        }
    }

    /// The JSON body for a task.
    #[must_use]
    pub fn request_body(&self, task: &ThinkerTask) -> serde_json::Value {
        json!({
            "model": self.model,
            "messages": [
                { "role": "system", "content": task.system() },
                { "role": "user", "content": task.prompt() },
            ],
            "language": self.language,
        })
    }
}

#[async_trait::async_trait]
impl Thinker for HttpThinker {
    async fn complete(&self, task: &ThinkerTask) -> Result<ThinkerReply, CoreError> {
        let mut request = reqwest::Client::new()
            .post(self.request_url())
            .json(&self.request_body(task));
        if let Some(credential) = &self.credential {
            request = request.bearer_auth(credential);
        }
        let response = request.send().await.map_err(|error| {
            CoreError::ProviderUnavailable(format!(
                "thinker request to {} failed: {error}",
                self.request_url()
            ))
        })?;
        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(CoreError::ProviderUnavailable(format!(
                "thinker endpoint returned {status}: {}",
                detail.trim()
            )));
        }
        let body: ChatResponse = response.json().await.map_err(|error| {
            CoreError::ProviderUnavailable(format!("thinker response was not JSON: {error}"))
        })?;
        let text = body
            .choices
            .first()
            .map(|choice| choice.message.content.trim().to_owned())
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                CoreError::ProviderUnavailable("thinker returned no content".to_owned())
            })?;
        Ok(ThinkerReply {
            text,
            provider: self.provider().to_owned(),
        })
    }

    fn provider(&self) -> &'static str {
        "http-thinker"
    }
}

/// The thinker choice a session resolved to.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct ThinkerSelection {
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
    /// Language sent to the provider.
    pub language: String,
}

impl ThinkerSelection {
    /// The embedded offline choice.
    #[must_use]
    pub fn embedded() -> Self {
        Self {
            provider: "embedded-thinker".to_owned(),
            kind: ProviderKind::Embedded,
            model: "embedded-template".to_owned(),
            endpoint: None,
            credentials_ref: None,
            language: "en".to_owned(),
        }
    }

    /// `true` when prompts leave this computer.
    #[must_use]
    pub const fn is_remote(&self) -> bool {
        matches!(self.kind, ProviderKind::SelfHosted | ProviderKind::Cloud)
    }
}

/// Resolve which thinker a session should use.
///
/// Precedence: saved configuration id, then an inline
/// `session.configuration.thinker` object, then [`ThinkerSelection::embedded`].
///
/// # Errors
///
/// Returns [`CoreError::InvalidConfiguration`] when the selection is unknown,
/// incomplete, or remote without consent.
pub fn select_thinker(
    session: &Session,
    configurations: &[AiConfiguration],
) -> Result<ThinkerSelection, CoreError> {
    let configured = session.configuration.get("thinker");
    if let Some(serde_json::Value::String(id)) = configured {
        if let Ok(id) = uuid::Uuid::parse_str(id) {
            if let Some(configuration) = configurations.iter().find(|item| item.id == id) {
                return thinker_from_config(&configuration.thinker);
            }
        }
    }
    if let Some(value) = configured.filter(|value| value.is_object()) {
        let config: ProviderConfig = serde_json::from_value(value.clone()).map_err(|error| {
            CoreError::InvalidConfiguration(format!("thinker config is invalid: {error}"))
        })?;
        return thinker_from_config(&config);
    }
    Ok(ThinkerSelection::embedded())
}

/// Resolve a one-off model against a saved configuration for that provider.
///
/// The first configuration whose thinker provider matches is cloned, its model
/// is replaced, and the result is validated. This is how a chat request picks
/// `provider:model` without rewriting the session configuration.
///
/// # Errors
///
/// Returns [`CoreError::InvalidConfiguration`] when no saved configuration uses
/// `provider`, and the usual thinker errors when the overridden slot is incomplete.
pub fn select_model(
    provider: &str,
    model: &str,
    configurations: &[AiConfiguration],
) -> Result<ThinkerSelection, CoreError> {
    let provider = provider.trim();
    let model = model.trim();
    let Some(saved) = configurations
        .iter()
        .find(|item| item.thinker.provider.trim() == provider)
    else {
        return Err(CoreError::InvalidConfiguration(format!(
            "provider '{provider}' is not configured"
        )));
    };
    let mut config = saved.thinker.clone();
    config.model = model.to_owned();
    thinker_from_config(&config)
}

/// Validate one stored thinker slot against the provider registry.
///
/// # Errors
///
/// Returns [`CoreError`] for an unknown provider or an incomplete remote slot.
pub fn thinker_from_config(config: &ProviderConfig) -> Result<ThinkerSelection, CoreError> {
    let provider = config.provider.trim();
    let Some(known_kind) = crate::ai::provider_kind(crate::ai::Capability::Thinker, provider)
    else {
        return Err(CoreError::UnknownProvider(provider.to_owned()));
    };
    if config.kind != known_kind {
        return Err(CoreError::InvalidConfiguration(format!(
            "thinker provider '{provider}' requires kind {}",
            known_kind.as_str()
        )));
    }
    let model = config.model.trim();
    if model.is_empty() {
        return Err(CoreError::InvalidConfiguration(
            "thinker model must not be empty".to_owned(),
        ));
    }
    let endpoint = config
        .endpoint
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty());
    let remote = matches!(known_kind, ProviderKind::SelfHosted | ProviderKind::Cloud);
    if remote {
        let endpoint = endpoint.ok_or_else(|| {
            CoreError::InvalidConfiguration(format!("thinker endpoint is required for {provider}"))
        })?;
        if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
            return Err(CoreError::InvalidConfiguration(
                "thinker endpoint must start with http:// or https://".to_owned(),
            ));
        }
        if !config.consent {
            return Err(CoreError::InvalidConfiguration(
                "thinker provider needs consent before session text leaves this computer"
                    .to_owned(),
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
            "thinker credentials_ref is required for cloud".to_owned(),
        ));
    }
    Ok(ThinkerSelection {
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
/// # Errors
///
/// Returns [`CoreError::InvalidConfiguration`] when a remote selection is
/// incomplete or a cloud selection has no credential.
pub fn build_thinker(
    selection: &ThinkerSelection,
    credential: Option<String>,
) -> Result<Arc<dyn Thinker>, CoreError> {
    match selection.kind {
        ProviderKind::Embedded => Ok(Arc::new(EmbeddedThinker)),
        ProviderKind::SelfHosted | ProviderKind::Cloud => {
            let endpoint = selection.endpoint.clone().ok_or_else(|| {
                CoreError::InvalidConfiguration(format!(
                    "thinker endpoint is required for {}",
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
            Ok(Arc::new(HttpThinker {
                endpoint,
                model: selection.model.clone(),
                language: selection.language.clone(),
                credential,
            }))
        }
    }
}

/// Resolves the thinker a session should use.
#[async_trait::async_trait]
pub trait ThinkerResolver: Send + Sync + 'static {
    /// Build the thinker for `session`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError`] when the selection is invalid or its credential is
    /// unavailable.
    async fn resolve(&self, session: &Session) -> Result<Arc<dyn Thinker>, CoreError>;
}

/// Always returns the embedded offline thinker.
#[derive(Debug, Default, Clone, Copy)]
pub struct EmbeddedThinkerResolver;

#[async_trait::async_trait]
impl ThinkerResolver for EmbeddedThinkerResolver {
    async fn resolve(&self, _session: &Session) -> Result<Arc<dyn Thinker>, CoreError> {
        Ok(Arc::new(EmbeddedThinker))
    }
}

/// Always returns one fixed thinker, ignoring the session.
#[derive(Clone)]
pub struct FixedThinkerResolver {
    thinker: Arc<dyn Thinker>,
}

impl FixedThinkerResolver {
    /// Wrap one thinker.
    #[must_use]
    pub fn new(thinker: Arc<dyn Thinker>) -> Self {
        Self { thinker }
    }
}

#[async_trait::async_trait]
impl ThinkerResolver for FixedThinkerResolver {
    async fn resolve(&self, _session: &Session) -> Result<Arc<dyn Thinker>, CoreError> {
        Ok(Arc::clone(&self.thinker))
    }
}

/// Resolves a session's thinker from saved AI configurations.
#[derive(Clone)]
pub struct ConfiguredThinkerResolver {
    repository: Arc<dyn crate::ai::AiConfigurationRepository>,
}

impl ConfiguredThinkerResolver {
    /// Wrap an AI configuration repository.
    #[must_use]
    pub fn new(repository: Arc<dyn crate::ai::AiConfigurationRepository>) -> Self {
        Self { repository }
    }
}

#[async_trait::async_trait]
impl ThinkerResolver for ConfiguredThinkerResolver {
    async fn resolve(&self, session: &Session) -> Result<Arc<dyn Thinker>, CoreError> {
        let configurations = self.repository.list().await?;
        let selection = select_thinker(session, &configurations)?;
        let credential = selection
            .credentials_ref
            .as_deref()
            .and_then(crate::audio::credential_from_env);
        build_thinker(&selection, credential)
    }
}
