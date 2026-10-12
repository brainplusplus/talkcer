//! Desktop shell boundary.
//!
//! Overlay visibility, position, and click-through are planned here. The shell
//! is a client of the core API: it never captures audio and never talks to a
//! provider directly. Opening a native window needs the Tauri runtime, which is
//! behind the optional `tauri-shell` feature so the default workspace build
//! stays headless.

pub mod client;
pub mod hotkeys;
pub mod overlay;
pub mod shell;
pub mod stealth;
pub mod subscription;
pub mod tray;

/// Native Tauri shell, built only with the `tauri-shell` feature.
#[cfg(feature = "tauri-shell")]
pub mod tauri_shell;

/// Windows capture-exclusion implementation of undetectable mode.
#[cfg(all(feature = "tauri-shell", windows))]
pub mod win_stealth;

/// Crate version, matching the workspace package version.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Default API the desktop shell attaches to.
pub const DEFAULT_API: &str = "http://127.0.0.1:54000/api";

/// Confirm the desktop crate can see the server contract.
#[must_use]
pub fn server_bind() -> &'static str {
    talkcer_server::DEFAULT_BIND
}
