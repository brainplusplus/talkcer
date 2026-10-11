//! Pairing gate for a second computer.
//!
//! Remote access is disabled until pairing creates a token. The database stores
//! only the SHA-256 hash. The raw token is returned once.

use chrono::Utc;
use getrandom::getrandom;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{Row, SqlitePool};

use crate::error::CoreError;

const ACCESS_ID: &str = "default";

/// Stored remote-access state. The token itself is never included.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RemoteStatus {
    /// `true` only after a successful pairing.
    pub enabled: bool,
}

/// Pairing response. `token` is present only when access is enabled.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Pairing {
    /// Whether remote clients may connect.
    pub enabled: bool,
    /// Shown once. Later checks use its hash.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub token: Option<String>,
}

/// Body of `POST /api/remote/pair`.
#[derive(Debug, Clone, Deserialize)]
pub struct PairRequest {
    /// `true` creates a token. `false` removes it.
    pub enabled: bool,
}

/// Pairing store.
#[derive(Clone)]
pub struct RemoteAccessService {
    pool: SqlitePool,
}

impl RemoteAccessService {
    /// Use the migrated pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }

    /// Return whether remote access is enabled.
    ///
    /// # Errors
    ///
    /// Returns a database error when the row cannot be read.
    pub async fn status(&self) -> Result<RemoteStatus, CoreError> {
        Ok(RemoteStatus { enabled: self.token_hash().await?.is_some() })
    }

    /// Enable or disable remote access.
    ///
    /// # Errors
    ///
    /// Returns a database error when the row cannot be saved.
    pub async fn pair(&self, enabled: bool) -> Result<Pairing, CoreError> {
        let now = Utc::now().to_rfc3339();
        if !enabled {
            self.save(false, None, &now).await?;
            return Ok(Pairing { enabled: false, token: None });
        }
        let token = random_token()?;
        self.save(true, Some(hash_token(&token)), &now).await?;
        Ok(Pairing { enabled: true, token: Some(token) })
    }

    /// Check a bearer token from a second computer.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::Unauthorized`] when access is disabled or the token differs.
    pub async fn authorize(&self, token: &str) -> Result<(), CoreError> {
        match self.token_hash().await? {
            Some(expected) if expected == hash_token(token) => Ok(()),
            _ => Err(CoreError::Unauthorized),
        }
    }

    async fn token_hash(&self) -> Result<Option<String>, CoreError> {
        let row = sqlx::query("SELECT enabled, token_hash FROM remote_access WHERE id = ?")
            .bind(ACCESS_ID)
            .fetch_optional(&self.pool)
            .await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let enabled: i64 = row.try_get("enabled")?;
        let hash: Option<String> = row.try_get("token_hash")?;
        Ok((enabled != 0).then_some(hash).flatten())
    }

    async fn save(&self, enabled: bool, hash: Option<String>, now: &str) -> Result<(), CoreError> {
        sqlx::query(
            "INSERT INTO remote_access (id, enabled, token_hash, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?)
             ON CONFLICT(id) DO UPDATE SET
                enabled = excluded.enabled,
                token_hash = excluded.token_hash,
                updated_at = excluded.updated_at",
        )
        .bind(ACCESS_ID)
        .bind(i64::from(enabled))
        .bind(hash)
        .bind(now)
        .bind(now)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn random_token() -> Result<String, CoreError> {
    let mut bytes = [0_u8; 32];
    getrandom(&mut bytes).map_err(|error| CoreError::Migration(format!("pairing token: {error}")))?;
    Ok(bytes.iter().map(|byte| format!("{byte:02x}")).collect())
}

fn hash_token(token: &str) -> String {
    let digest = Sha256::digest(token.as_bytes());
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}
