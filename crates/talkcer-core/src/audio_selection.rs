//! The microphone, playback device, and per-channel speech model capture uses.
//!
//! One row holds all four values, so the settings page and the capture pipeline
//! read the same source. An empty device means the system default. An empty
//! model means the session's own STT choice.
//!
//! The row is cached in memory because capture opens devices from a synchronous
//! provider call. The cache is filled at startup and updated on every save, so
//! a restart keeps the choice instead of silently returning to the default.

use std::sync::{Arc, RwLock};

use chrono::Utc;
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};

use crate::error::CoreError;

/// The audio inputs capture should use.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct AudioSelection {
    /// Capture device id, or empty for the system default.
    #[serde(default)]
    pub microphone: String,
    /// Playback device id captured in loopback, or empty for the default.
    #[serde(default)]
    pub playback: String,
    /// `provider:model` that reads the microphone, or empty for the session's.
    #[serde(default)]
    pub microphone_model: String,
    /// `provider:model` that reads system audio, or empty for the session's.
    #[serde(default)]
    pub system_model: String,
}

impl AudioSelection {
    /// The model chosen for one capture channel.
    ///
    /// `channel` is `microphone` or `system_audio`; any other label reads the
    /// microphone value so an unexpected channel never drops the choice.
    #[must_use]
    pub fn model_for(&self, channel: &str) -> Option<&str> {
        let value = if channel == "system_audio" {
            self.system_model.trim()
        } else {
            self.microphone_model.trim()
        };
        (!value.is_empty()).then_some(value)
    }
}

/// Reads, writes, and caches the single audio selection row.
#[derive(Clone)]
pub struct AudioSelectionService {
    pool: SqlitePool,
    cached: Arc<RwLock<AudioSelection>>,
}

impl AudioSelectionService {
    /// Create a service over the shared pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self {
            pool,
            cached: Arc::new(RwLock::new(AudioSelection::default())),
        }
    }

    /// Read the row into the cache. Call once at startup.
    ///
    /// # Errors
    ///
    /// Returns a database error when the row cannot be read.
    pub async fn load(&self) -> Result<AudioSelection, CoreError> {
        let selection = self.read().await?;
        if let Ok(mut cache) = self.cached.write() {
            *cache = selection.clone();
        }
        Ok(selection)
    }

    /// The last loaded or saved selection. Never touches the database, so it is
    /// safe to call from a synchronous device provider.
    #[must_use]
    pub fn current(&self) -> AudioSelection {
        self.cached
            .read()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }

    /// The stored selection, read from the database.
    ///
    /// # Errors
    ///
    /// Returns a database error when the row cannot be read.
    pub async fn get(&self) -> Result<AudioSelection, CoreError> {
        self.read().await
    }

    async fn read(&self) -> Result<AudioSelection, CoreError> {
        let row = sqlx::query(
            "SELECT microphone, playback, microphone_model, system_model
             FROM audio_selection WHERE id = 'audio'",
        )
        .fetch_optional(&self.pool)
        .await?;
        let Some(row) = row else {
            return Ok(AudioSelection::default());
        };
        Ok(AudioSelection {
            microphone: row.try_get("microphone")?,
            playback: row.try_get("playback")?,
            microphone_model: row.try_get("microphone_model")?,
            system_model: row.try_get("system_model")?,
        })
    }

    /// Insert or replace the selection, refresh the cache, and return what was
    /// stored.
    ///
    /// Values are trimmed. An empty value means "system default" for a device
    /// and "session choice" for a model.
    ///
    /// # Errors
    ///
    /// Returns a database error when the write fails.
    pub async fn save(&self, selection: AudioSelection) -> Result<AudioSelection, CoreError> {
        let cleaned = AudioSelection {
            microphone: selection.microphone.trim().to_owned(),
            playback: selection.playback.trim().to_owned(),
            microphone_model: selection.microphone_model.trim().to_owned(),
            system_model: selection.system_model.trim().to_owned(),
        };
        sqlx::query(
            "INSERT INTO audio_selection
                (id, microphone, playback, microphone_model, system_model, updated_at)
             VALUES ('audio', ?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(id) DO UPDATE SET
                microphone = excluded.microphone,
                playback = excluded.playback,
                microphone_model = excluded.microphone_model,
                system_model = excluded.system_model,
                updated_at = excluded.updated_at",
        )
        .bind(&cleaned.microphone)
        .bind(&cleaned.playback)
        .bind(&cleaned.microphone_model)
        .bind(&cleaned.system_model)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        if let Ok(mut cache) = self.cached.write() {
            *cache = cleaned.clone();
        }
        Ok(cleaned)
    }
}
