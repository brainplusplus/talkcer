//! Talkcer core: domain types, persistence, and session services.

pub mod domain;
pub mod error;
pub mod migrate;
pub mod session;

pub use domain::{Session, SessionMode, SessionStatus};
pub use error::CoreError;
pub use migrate::{connect_and_migrate, embedded_migrations, MigrationSource};
pub use session::{
    CreateSession, SessionFilter, SessionRepository, SessionService, SessionsByMode,
    SqliteSessionRepository, UpdateSession,
};
