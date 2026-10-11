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

    /// A chat message is empty after trimming.
    #[error("message text must not be empty")]
    EmptyText,

    /// No session exists for the given id.
    #[error("session not found")]
    NotFound,

    /// No timeline item exists for the given id in this session.
    #[error("timeline item not found")]
    ItemNotFound,

    /// A timeline speaker is outside the wire contract.
    #[error("invalid speaker: {0}")]
    InvalidSpeaker(String),

    /// A timeline source is outside the wire contract.
    #[error("invalid source: {0}")]
    InvalidSource(String),

    /// A timeline item kind is outside the wire contract.
    #[error("invalid item kind: {0}")]
    InvalidItemKind(String),

    /// A timeline purpose is outside the wire contract.
    #[error("invalid purpose: {0}")]
    InvalidPurpose(String),

    /// An assistant action name is outside the wire contract.
    #[error("invalid assistant action: {0}")]
    InvalidAction(String),

    /// An assistant target is missing or not an utterance.
    #[error("assistant target must be an utterance")]
    InvalidTarget,

    /// `started_at_ms` is negative.
    #[error("started_at_ms must be zero or greater")]
    InvalidTimestamp,

    /// `limit` is outside the accepted range.
    #[error("limit must be between 1 and 500")]
    InvalidLimit,

    /// A final utterance cannot be changed.
    #[error("utterance is final and cannot be updated")]
    UtteranceClosed,

    /// The utterance id belongs to a different session.
    #[error("utterance belongs to another session")]
    UtteranceSessionMismatch,

    /// A provider kind is outside `embedded`, `self_hosted`, and `cloud`.
    #[error("invalid provider kind: {0}")]
    InvalidProviderKind(String),

    /// A provider configuration is missing a required field or contains a secret.
    #[error("invalid provider configuration: {0}")]
    InvalidConfiguration(String),

    /// A provider name is not registered for that capability.
    #[error("unknown provider: {0}")]
    UnknownProvider(String),

    /// A capability name is outside `stt`, `thinker`, and `tts`.
    #[error("invalid capability: {0}")]
    InvalidCapability(String),

    /// A configuration name is empty after trimming.
    #[error("configuration name must not be empty")]
    EmptyName,

    /// The action is not available for this session mode or status.
    #[error("invalid mock interview action: {0}")]
    InvalidMockAction(String),

    /// The action is not available for this learning session.
    #[error("invalid learning action: {0}")]
    InvalidLearningAction(String),

    /// Browser audio input cannot be transcribed.
    #[error("invalid audio input: {0}")]
    InvalidAudio(String),

    /// A live capture action is not legal in the current state.
    #[error("invalid capture action: {0}")]
    InvalidCapture(String),

    /// A native audio device could not be enumerated, opened, or read.
    #[error("audio device error: {0}")]
    AudioDevice(String),

    /// The audio pipeline could not turn a speech run into text.
    #[error("transcription failed: {0}")]
    Transcription(String),

    /// Remote access is disabled or the pairing token is wrong.
    #[error("remote access denied")]
    Unauthorized,

    /// The remote client sent too many requests.
    #[error("remote rate limit exceeded")]
    RateLimited,

    /// An overlay change is not legal in the current state.
    #[error("invalid overlay action: {0}")]
    InvalidOverlay(String),

    /// The export format is not supported.
    #[error("invalid export format: {0}")]
    InvalidExport(String),

    /// A provider call did not finish in time.
    #[error("provider timed out")]
    ProviderTimeout,

    /// Persistence failed.
    #[error("database error: {0}")]
    Database(#[from] sqlx::Error),

    /// A migration file could not be applied.
    #[error("migration error: {0}")]
    Migration(String),
}
