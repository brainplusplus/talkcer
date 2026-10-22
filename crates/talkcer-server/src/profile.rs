//! Profile routes.
//!
//! The CV upload reads the file and drops it. When a thinker is configured it
//! fills the fields; otherwise it returns the raw text for the user to apply.

use std::sync::Arc;

use axum::extract::{Multipart, State};
use axum::{Json, Router};
use talkcer_core::{
    build_thinker, credential_from_env, parse_cv_fields, read_cv, thinker_from_config, CvDraft,
    CvSource, Profile, SaveProfile, Thinker, ThinkerTask,
};

use crate::error::ApiError;
use crate::state::AppState;

/// Profile routes.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/api/profile", axum::routing::get(get).put(save))
        .route("/api/profile/cv", axum::routing::post(cv))
}

async fn get(State(state): State<AppState>) -> Result<Json<Profile>, ApiError> {
    Ok(Json(state.profile.get().await?))
}

async fn save(
    State(state): State<AppState>,
    Json(input): Json<SaveProfile>,
) -> Result<Json<Profile>, ApiError> {
    Ok(Json(state.profile.save(input).await?))
}

async fn cv(State(state): State<AppState>, mut form: Multipart) -> Result<Json<CvDraft>, ApiError> {
    let mut filename = String::from("cv");
    let mut bytes = Vec::new();
    while let Some(field) =
        form.next_field()
            .await
            .map_err(|error: axum::extract::multipart::MultipartError| {
                talkcer_core::CoreError::InvalidProfile(error.to_string())
            })?
    {
        if field.name() != Some("file") {
            continue;
        }
        filename = field.file_name().unwrap_or("cv").to_owned();
        bytes = field
            .bytes()
            .await
            .map_err(|error: axum::extract::multipart::MultipartError| {
                talkcer_core::CoreError::InvalidProfile(error.to_string())
            })?
            .to_vec();
    }
    let mut draft = read_cv(&filename, &bytes)?;
    if draft.warning.is_some() {
        return Ok(Json(draft));
    }
    match configured_thinker(&state).await {
        Some(thinker) => match thinker
            .complete(&ThinkerTask::ScreenContext {
                prompt: talkcer_core::cv_prompt(&draft.text),
            })
            .await
        {
            Ok(reply) => {
                let fields = parse_cv_fields(&reply.text);
                draft = CvDraft {
                    text: serde_json::to_string(&fields).unwrap_or_default(),
                    source: CvSource::LanguageModel,
                    warning: None,
                };
            }
            Err(error) => {
                draft.warning = Some(format!(
                    "the model could not read the file, so the raw text is shown instead: {error}"
                ));
            }
        },
        None => {
            draft.warning =
                Some("no model is configured, so the raw text is shown instead".to_owned());
        }
    }
    Ok(Json(draft))
}

/// The first configured thinker, if one exists.
///
/// Profile autofill has no session of its own, so it uses the earliest saved
/// configuration. An embedded configuration is skipped: it returns template
/// text, which is useless for filling a form.
async fn configured_thinker(state: &AppState) -> Option<Arc<dyn Thinker>> {
    let configs = state.ai.list().await.ok()?;
    let config = configs.first()?;
    let selection = thinker_from_config(&config.thinker).ok()?;
    if !selection.is_remote() {
        return None;
    }
    let credential = selection
        .credentials_ref
        .as_deref()
        .and_then(credential_from_env);
    build_thinker(&selection, credential).ok()
}
