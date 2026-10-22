//! Which models a capability offers in the session picker.
//!
//! The catalogs say what a provider *can* run. A selection says which of those
//! models the user has ticked, which providers are switched off, and which
//! model is the default for a provider. One row per capability.

use std::collections::BTreeMap;

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::error::CoreError;

/// One model ticked for the session picker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct OfferedModel {
    /// Registered provider name.
    pub provider: String,
    /// Model identifier.
    pub model: String,
}

/// The saved picker state for one capability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProviderSelection {
    /// `stt`, `thinker`, or `tts`.
    pub capability: String,
    /// Models ticked for the session picker.
    pub offered: Vec<OfferedModel>,
    /// Provider ids switched off.
    pub disabled: Vec<String>,
    /// Provider name to its default model id.
    pub defaults: BTreeMap<String, String>,
}

impl ProviderSelection {
    /// A selection with nothing ticked and nothing switched off.
    #[must_use]
    pub fn empty(capability: impl Into<String>) -> Self {
        Self {
            capability: capability.into(),
            offered: Vec::new(),
            disabled: Vec::new(),
            defaults: BTreeMap::new(),
        }
    }
}

/// Loads and stores the three provider selections.
#[derive(Clone)]
pub struct SelectionService {
    pool: SqlitePool,
}

impl SelectionService {
    /// Create a service over the shared pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Return the saved selection, or an empty one when the row does not exist.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidConfiguration`] for an unknown capability and
    /// a database error when the row cannot be read.
    pub async fn get(&self, capability: &str) -> Result<ProviderSelection, CoreError> {
        let capability = known_capability(capability)?;
        let row = sqlx::query(
            "SELECT offered, disabled, defaults FROM provider_selections WHERE capability = ?1",
        )
        .bind(capability)
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(ProviderSelection::empty(capability));
        };
        Ok(ProviderSelection {
            capability: capability.to_owned(),
            offered: parse_offered(row.try_get("offered")?),
            disabled: parse_disabled(row.try_get("disabled")?),
            defaults: parse_defaults(row.try_get("defaults")?),
        })
    }

    /// Every capability, including ones that have never been saved.
    ///
    /// # Errors
    ///
    /// Returns a database error when a row cannot be read.
    pub async fn list(&self) -> Result<Vec<ProviderSelection>, CoreError> {
        let mut selections = Vec::with_capacity(3);
        for capability in ["stt", "thinker", "tts"] {
            selections.push(self.get(capability).await?);
        }
        Ok(selections)
    }

    /// Insert or replace one selection and return the stored value.
    ///
    /// Provider and model names are trimmed. Empty entries are dropped and
    /// duplicates are removed.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidConfiguration`] when the capability is not
    /// `stt`, `thinker`, or `tts`, and a database error when the write fails.
    pub async fn save(&self, selection: ProviderSelection) -> Result<ProviderSelection, CoreError> {
        let capability = known_capability(&selection.capability)?.to_owned();
        let cleaned = clean(selection);
        let now = Utc::now().to_rfc3339();
        sqlx::query(
            "INSERT INTO provider_selections (capability, offered, disabled, defaults, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(capability) DO UPDATE SET
                offered = excluded.offered,
                disabled = excluded.disabled,
                defaults = excluded.defaults,
                updated_at = excluded.updated_at",
        )
        .bind(&capability)
        .bind(serde_json::to_string(&cleaned.offered).unwrap_or_else(|_| "[]".to_owned()))
        .bind(serde_json::to_string(&cleaned.disabled).unwrap_or_else(|_| "[]".to_owned()))
        .bind(serde_json::to_string(&cleaned.defaults).unwrap_or_else(|_| "{}".to_owned()))
        .bind(now)
        .execute(&self.pool)
        .await?;
        self.get(&capability).await
    }
}

fn known_capability(capability: &str) -> Result<&str, CoreError> {
    match capability.trim() {
        "stt" => Ok("stt"),
        "thinker" => Ok("thinker"),
        "tts" => Ok("tts"),
        other => Err(CoreError::InvalidConfiguration(format!(
            "capability '{other}' is not stt, thinker, or tts"
        ))),
    }
}

fn clean(selection: ProviderSelection) -> ProviderSelection {
    let mut offered = Vec::new();
    for item in selection.offered {
        let provider = item.provider.trim().to_owned();
        let model = item.model.trim().to_owned();
        if provider.is_empty() || model.is_empty() {
            continue;
        }
        if offered
            .iter()
            .any(|existing: &OfferedModel| existing.provider == provider && existing.model == model)
        {
            continue;
        }
        offered.push(OfferedModel { provider, model });
    }

    let mut disabled = Vec::new();
    for provider in selection.disabled {
        let provider = provider.trim().to_owned();
        if provider.is_empty()
            || disabled
                .iter()
                .any(|existing: &String| existing == &provider)
        {
            continue;
        }
        disabled.push(provider);
    }

    let mut defaults = BTreeMap::new();
    for (provider, model) in selection.defaults {
        let provider = provider.trim().to_owned();
        let model = model.trim().to_owned();
        if provider.is_empty() || model.is_empty() {
            continue;
        }
        defaults.insert(provider, model);
    }

    ProviderSelection {
        capability: selection.capability,
        offered,
        disabled,
        defaults,
    }
}

fn parse_offered(value: String) -> Vec<OfferedModel> {
    serde_json::from_str(&value).unwrap_or_default()
}

fn parse_disabled(value: String) -> Vec<String> {
    serde_json::from_str(&value).unwrap_or_default()
}

fn parse_defaults(value: String) -> BTreeMap<String, String> {
    serde_json::from_str(&value).unwrap_or_default()
}
