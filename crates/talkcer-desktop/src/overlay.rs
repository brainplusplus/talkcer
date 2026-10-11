//! Headless plan for the native overlay window.
//!
//! The commands describe what a Tauri window must do. Tests assert the plan
//! without opening a window.

use talkcer_core::{OverlayPreferences, OverlayState};

/// One native window operation.
#[derive(Debug, Clone, PartialEq)]
pub enum WindowCommand {
    /// Show a compact window above other windows.
    Show { always_on_top: bool, opacity: f64, x: i64, y: i64, width: i64, height: i64 },
    /// Hide the window completely.
    Hide,
    /// Let empty overlay space ignore the pointer.
    IgnoreCursor(bool),
}

/// Shortcut the tray and keyboard hook should bind to the overlay toggle.
pub const OVERLAY_SHORTCUT: &str = "Ctrl+Shift+Space";

/// Turn saved preferences into window commands.
#[must_use]
pub fn plan(preferences: &OverlayPreferences) -> Vec<WindowCommand> {
    match preferences.state {
        OverlayState::Hidden => vec![WindowCommand::Hide],
        OverlayState::Visible => vec![WindowCommand::Show {
            always_on_top: preferences.always_on_top,
            opacity: preferences.opacity,
            x: preferences.position_x,
            y: preferences.position_y,
            width: preferences.width,
            height: preferences.height,
        }],
        OverlayState::ClickThrough => vec![
            WindowCommand::Show {
                always_on_top: preferences.always_on_top,
                opacity: preferences.opacity,
                x: preferences.position_x,
                y: preferences.position_y,
                width: preferences.width,
                height: preferences.height,
            },
            WindowCommand::IgnoreCursor(true),
        ],
    }
}
