//! Typed errors for domain validation and persistence.

use thiserror::Error;

/// Recoverable failure from Talkcer core.
#[derive(Debug, Error)]
pub enum CoreError {
    /// A session mode string is not one of the allowed values.
    #[error("invalid session mode: {0}")]
    InvalidMode(String),

    /// A list query mixes `grouped=true` with mode or status filters.
    #[error("grouped listing cannot be combined with mode or status filters")]
    GroupedFilterConflict,

    /// A session status string is not one of the allowed values.
    #[error("invalid session status: {0}")]
    InvalidStatus(String),

    /// The requested status change is not a legal session transition.
    #[error("invalid session transition: {from} -> {to}")]
    InvalidTransition {
        /// Status before the change.
        from: &'static str,
        /// Requested status.
        to: &'static str,
    },

    /// A session title is empty after trimming.
    #[error("session title must not be empty")]
    EmptyTitle,

    /// No session exists for the given id.
    #[error("session not found")]
    NotFound,

    /// Persistence failed.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    /// A migration file could not be applied.
    #[error("migration error: {0}")]
    Migration(String),
}
