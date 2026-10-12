//! Headless plan for the native overlay window.
//!
//! The commands describe what a Tauri window must do. Tests assert the plan
//! without opening a window, so the same logic runs in CI and on a desktop.

use serde::Serialize;
use talkcer_core::{OverlayPreferences, OverlayState};

/// One native window operation.
///
/// The shell applies these in order. Each variant maps to one Tauri call:
/// `Show` to `show` plus `set_always_on_top`, `set_size`, `set_position`, and
/// the window background alpha; `Hide` to `hide`; `IgnoreCursor` to
/// `set_ignore_cursor_events`.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "snake_case", tag = "command")]
pub enum WindowCommand {
    /// Show a compact window above other windows.
    Show {
        /// Stay above other windows while visible.
        always_on_top: bool,
        /// Window opacity from 0.2 through 1.
        opacity: f64,
        /// Horizontal position.
        x: i64,
        /// Vertical position.
        y: i64,
        /// Window width.
        width: i64,
        /// Window height.
        height: i64,
    },
    /// Hide the window completely.
    Hide,
    /// Let empty overlay space ignore the pointer.
    IgnoreCursor(bool),
}

/// Shortcut the tray binds to the overlay toggle.
///
/// One of the five global combos; the full table lives in [`crate::stealth`].
pub const OVERLAY_SHORTCUT: &str = crate::stealth::shortcuts::OVERLAY_TOGGLE;

/// Turn saved preferences into window commands.
///
/// A hidden overlay always plans exactly one `Hide`, so it never keeps a
/// visible window that could intercept clicks. A visible overlay is always
/// on top; click-through adds the pointer pass-through command.
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

/// The overlay states reachable from `current`.
///
/// Mirrors the overlay state machine in the technical docs:
/// `Hidden <-> Visible <-> ClickThrough`, plus `Visible -> Closed`.
#[must_use]
pub fn next_states(current: OverlayState) -> &'static [OverlayState] {
    match current {
        OverlayState::Hidden => &[OverlayState::Visible],
        OverlayState::Visible => &[OverlayState::Hidden, OverlayState::ClickThrough],
        OverlayState::ClickThrough => &[OverlayState::Visible, OverlayState::Hidden],
    }
}

/// Whether the transition is allowed by the overlay state machine.
#[must_use]
pub fn can_transition(from: OverlayState, to: OverlayState) -> bool {
    next_states(from).contains(&to)
}

/// The window command that hides the overlay, used by the tray and the shortcut.
#[must_use]
pub fn hidden_plan() -> Vec<WindowCommand> {
    vec![WindowCommand::Hide]
}
