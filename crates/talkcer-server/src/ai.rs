//! AI configuration HTTP routes.
//!
//! Secrets stay out of the stored configuration. Provider tests only check
//! the saved configuration against the registry.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};
use talkcer_core::{
    catalog_model, checked_endpoint, discover_models, download_state, finish, local_model, local_models,
    model_catalogs, AiConfiguration, Capability, CoreError, DownloadState, LocalModel,
    ModelCatalog, ModelOption, ProviderDescriptor, ProviderSelection, ProviderTestResult,
    SaveAiConfiguration,
};
use uuid::Uuid;

use crate::error::ApiError;
use crate::state::AppState;

/// Routes mounted under `/api/config/ai`.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/config/ai", get(list).put(save))
        .route("/api/config/ai/providers", get(providers))
        .route("/api/config/ai/models", get(models))
        .route(
            "/api/config/ai/selection",
            get(list_selection).put(save_selection),
        )
        .route("/api/config/ai/discover", get(discover))
        .route(
            "/api/config/ai/downloads",
            get(list_downloads).post(start_download),
        )
        .route("/api/config/ai/{id}/test", post(test))
        .route("/api/config/ai/probe", post(probe))
}

/// One local model and whether its file is on disk.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DownloadStatus {
    /// Capability the model serves.
    pub capability: Capability,
    /// Provider name.
    pub provider: &'static str,
    /// Model identifier.
    pub model: &'static str,
    /// Combined size of every file the model needs, in megabytes.
    pub size_mb: u32,
    /// `ready`, `partial`, or `missing`. Ready only when every file is ready.
    pub state: &'static str,
}

impl DownloadStatus {
    fn from_model(model: LocalModel, dir: &std::path::Path) -> Self {
        let ready = model
            .files
            .iter()
            .all(|file| download_state(dir, file.file_name) == DownloadState::Ready);
        let partial = model.files.iter().any(|file| {
            !matches!(download_state(dir, file.file_name), DownloadState::Missing)
        });
        let state = if ready {
            DownloadState::Ready
        } else if partial {
            DownloadState::Partial
        } else {
            DownloadState::Missing
        };
        Self {
            capability: model.capability,
            provider: model.provider,
            model: model.model,
            size_mb: model.files.iter().map(|file| file.size_mb).sum(),
            state: state.as_str(),
        }
    }
}

async fn list_downloads(State(state): State<AppState>) -> Json<Vec<DownloadStatus>> {
    Json(
        local_models()
            .iter()
            .copied()
            .map(|model| DownloadStatus::from_model(model, &state.models_dir))
            .collect(),
    )
}

#[derive(Debug, Deserialize)]
struct DownloadBody {
    capability: Capability,
    provider: String,
    model: String,
}

async fn start_download(
    State(state): State<AppState>,
    Json(body): Json<DownloadBody>,
) -> Result<(StatusCode, Json<DownloadStatus>), ApiError> {
    let model = local_model(body.capability, &body.provider, &body.model).ok_or_else(|| {
        CoreError::ModelNotFound {
            capability: body.capability.as_str().to_owned(),
            provider: body.provider.clone(),
            model: body.model.clone(),
        }
    })?;
    if model_ready(model, &state.models_dir) {
        return Ok((
            StatusCode::OK,
            Json(DownloadStatus::from_model(model, &state.models_dir)),
        ));
    }
    let _guard = DownloadGuard::acquire(&state.downloads, &body.provider, &body.model)?;
    // Fetch only the files that are not finished yet, so a retry continues.
    for file in model.files {
        if download_state(&state.models_dir, file.file_name) == DownloadState::Ready {
            continue;
        }
        let bytes = fetch_model(file.url).await?;
        finish(&state.models_dir, file.file_name, &bytes)?;
    }
    Ok((
        StatusCode::CREATED,
        Json(DownloadStatus::from_model(model, &state.models_dir)),
    ))
}

/// True only when every file the model needs has finished downloading.
fn model_ready(model: LocalModel, dir: &std::path::Path) -> bool {
    model
        .files
        .iter()
        .all(|file| download_state(dir, file.file_name) == DownloadState::Ready)
}

/// Hugging Face only: model URLs are hard-coded, and this rejects anything else.
fn allowed_model_url(url: &str) -> Result<(), CoreError> {
    let parsed = reqwest::Url::parse(url)
        .map_err(|error| CoreError::InvalidConfiguration(format!("model url is invalid: {error}")))?;
    if parsed.scheme() == "https" && parsed.host_str() == Some("huggingface.co") {
        Ok(())
    } else {
        Err(CoreError::InvalidConfiguration(
            "model downloads are restricted to https://huggingface.co".to_owned(),
        ))
    }
}

async fn fetch_model(url: &str) -> Result<Vec<u8>, CoreError> {
    allowed_model_url(url)?;
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10 * 60))
        .build()
        .map_err(|error| CoreError::InvalidConfiguration(error.to_string()))?;
    let response = client
        .get(url)
        .send()
        .await
        .map_err(|error| CoreError::InvalidConfiguration(error.to_string()))?
        .error_for_status()
        .map_err(|error| CoreError::InvalidConfiguration(error.to_string()))?;
    response
        .bytes()
        .await
        .map(|bytes| bytes.to_vec())
        .map_err(|error| CoreError::InvalidConfiguration(error.to_string()))
}

/// Removes one in-flight key when the download returns, including on failure.
struct DownloadGuard {
    downloads: Arc<Mutex<HashSet<String>>>,
    key: String,
}

impl DownloadGuard {
    fn acquire(
        downloads: &Arc<Mutex<HashSet<String>>>,
        provider: &str,
        model: &str,
    ) -> Result<Self, ApiError> {
        let key = format!("{provider}:{model}");
        let mut inflight = downloads
            .lock()
            .map_err(|_| CoreError::InvalidConfiguration("download lock poisoned".to_owned()))?;
        if !inflight.insert(key.clone()) {
            return Err(conflict());
        }
        drop(inflight);
        Ok(Self {
            downloads: Arc::clone(downloads),
            key,
        })
    }
}

impl Drop for DownloadGuard {
    fn drop(&mut self) {
        if let Ok(mut inflight) = self.downloads.lock() {
            inflight.remove(&self.key);
        }
    }
}

/// An in-flight download is a conflict.
fn conflict() -> ApiError {
    CoreError::DownloadInProgress.into()
}

async fn list(State(state): State<AppState>) -> Result<Json<Vec<AiConfiguration>>, ApiError> {
    Ok(Json(state.ai.list().await?))
}

async fn providers(State(state): State<AppState>) -> Json<Vec<ProviderDescriptor>> {
    Json(state.ai.providers().to_vec())
}

async fn models() -> Json<Vec<ModelCatalog>> {
    Json(model_catalogs())
}

async fn save(
    State(state): State<AppState>,
    Json(input): Json<SaveAiConfiguration>,
) -> Result<(StatusCode, Json<AiConfiguration>), ApiError> {
    let created = input.id.is_none();
    let saved = state.ai.save(input).await?;
    tracing::info!(configuration_id = %saved.id, created, "ai configuration stored");
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((status, Json(saved)))
}

#[derive(Debug, Deserialize)]
struct TestBody {
    capability: Capability,
}

async fn list_selection(
    State(state): State<AppState>,
) -> Result<Json<Vec<ProviderSelection>>, ApiError> {
    Ok(Json(state.selection.list().await?))
}

async fn save_selection(
    State(state): State<AppState>,
    Json(selection): Json<ProviderSelection>,
) -> Result<Json<ProviderSelection>, ApiError> {
    let saved = state.selection.save(selection).await?;
    tracing::info!(capability = %saved.capability, "provider selection stored");
    Ok(Json(saved))
}

#[derive(Debug, Deserialize)]
struct DiscoverQuery {
    endpoint: String,
    #[serde(default)]
    key: Option<String>,
}

/// Discover models from an OpenAI-compatible gateway on this machine.
///
/// A failure, including a gateway that is down, returns an empty list so the
/// settings page can say nothing was found instead of showing an error.
async fn discover(Query(query): Query<DiscoverQuery>) -> Result<Json<Vec<ModelOption>>, ApiError> {
    // A bad scheme or a non-loopback host is a client error. A gateway that
    // is down or returns an unexpected body is an empty list.
    checked_endpoint(&query.endpoint)?;
    match discover_models(&query.endpoint, query.key.as_deref()).await {
        Ok(models) => Ok(Json(models)),
        Err(error) => {
            tracing::warn!(error = %error, "model discovery failed");
            Ok(Json(Vec::new()))
        }
    }
}

#[derive(Debug, Deserialize)]
struct ProbeBody {
    capability: Capability,
    provider: String,
    #[serde(default)]
    endpoint: String,
    #[serde(default)]
    model: String,
    #[serde(default)]
    credentials_ref: Option<String>,
}

/// Probe a provider with the values currently typed into its card.
///
/// Nothing is saved. The call only checks that the endpoint answers, which is
/// what Natively's Test button does: a `/models` read for a language model, and
/// the smallest request the speech provider will accept for the other two.
async fn probe(
    State(state): State<AppState>,
    Json(body): Json<ProbeBody>,
) -> Result<Json<ProviderTestResult>, ApiError> {
    let model = if body.model.trim().is_empty() { "probe" } else { body.model.trim() };
    // Every local model is tested by its file, not by calling an endpoint. The
    // catalog lists more models than the downloader can fetch today, so a model
    // with no file mapping still reports that it is not downloaded rather than
    // falling through to the endpoint check and demanding an address.
    let local = catalog_model(&body.capability, body.provider.trim(), model)
        .filter(|option| option.local);
    if local.is_some() {
        let ready = local_model(body.capability, body.provider.trim(), model)
            .map(|model| model_ready(model, &state.models_dir))
            .unwrap_or(false);
        return Ok(Json(ProviderTestResult {
            capability: body.capability,
            provider: body.provider,
            ok: ready,
            message: if ready {
                format!("{model} is downloaded and ready.")
            } else {
                format!("{model} is not downloaded yet.")
            },
        }));
    }
    let result = probe_provider(&body.capability, body.provider.trim(), body.endpoint.trim(), model, body.credentials_ref.as_deref()).await;
    Ok(Json(result))
}

/// Contact one provider and report whether it answered.
///
/// A network failure or a refusal comes back as `ok: false` with the reason,
/// never as an HTTP error, so the card can show it inline.
async fn probe_provider(
    capability: &Capability,
    provider: &str,
    endpoint: &str,
    model: &str,
    credentials_ref: Option<&str>,
) -> ProviderTestResult {
    let fail = |message: String| ProviderTestResult {
        capability: *capability,
        provider: provider.to_owned(),
        ok: false,
        message,
    };
    if !endpoint.starts_with("http://") && !endpoint.starts_with("https://") {
        return fail("Endpoint must start with http:// or https://.".to_owned());
    }
    let secret = credentials_ref.and_then(talkcer_core::credential_from_env);
    if credentials_ref.is_some() && secret.is_none() {
        return fail(format!(
            "No secret named {} is set in the environment.",
            credentials_ref.unwrap_or_default()
        ));
    }
    let client = match reqwest::Client::builder().timeout(std::time::Duration::from_secs(15)).build() {
        Ok(client) => client,
        Err(error) => return fail(error.to_string()),
    };
    let base = endpoint.trim_end_matches('/');
    let (url, payload) = match capability {
        Capability::Thinker => (
            format!("{base}/models"),
            None,
        ),
        Capability::Stt => (
            format!("{base}/audio/transcriptions"),
            Some(serde_json::json!({ "model": model })),
        ),
        Capability::Tts => (
            format!("{base}/audio/speech"),
            Some(serde_json::json!({ "model": model, "input": "test", "voice": "alloy" })),
        ),
    };
    let mut request = match payload {
        Some(body) => client.post(url).json(&body),
        None => client.get(url),
    };
    if let Some(secret) = secret {
        request = request.bearer_auth(secret);
    }
    match request.send().await {
        Ok(response) if response.status().is_success() || response.status().as_u16() == 400 => ProviderTestResult {
            capability: *capability,
            provider: provider.to_owned(),
            ok: true,
            message: format!("{provider} answered."),
        },
        Ok(response) => fail(format!("{provider} returned {}.", response.status())),
        Err(error) => fail(format!("Could not reach {provider}: {error}")),
    }
}

async fn test(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
    Json(body): Json<TestBody>,
) -> Result<Json<ProviderTestResult>, ApiError> {
    let result = state.ai.test(id, body.capability).await?;
    tracing::info!(
        configuration_id = %id,
        capability = result.capability.as_str(),
        ok = result.ok,
        "ai provider tested"
    );
    Ok(Json(result))
}
