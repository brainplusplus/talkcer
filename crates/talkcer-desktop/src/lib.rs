//! Desktop shell boundary.
//!
//! Overlay visibility, position, and click-through are planned here. Opening the
//! Tauri window requires the Tauri CLI, which is not part of the default build.

pub mod overlay;

/// Crate version, matching the workspace package version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default API the desktop shell will attach to once Task 10 lands.
pub const DEFAULT_API: &str = "http://127.0.0.1:8787/api";

/// Confirm the desktop crate can see the server contract.
#[must_use]
pub fn server_bind() -> &'static str {
    talkcer_server::DEFAULT_BIND
}
