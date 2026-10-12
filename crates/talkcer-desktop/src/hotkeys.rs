//! Global shortcut table: which `Ctrl+N` combo maps to which shell action.
//!
//! Pure mapping so tests cover every combo without registering anything. The
//! Tauri layer registers the table and dispatches through [`ShortcutAction`].

use serde::Serialize;
use talkcer_core::SessionMode;

/// Everything a global shortcut can ask the shell to do.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ShortcutAction {
    /// Show or hide the whole desktop app (main window and overlay).
    ToggleApp,
    /// Toggle undetectable mode.
    ToggleUndetectable,
    /// Toggle the overlay window.
    ToggleOverlay,
    /// Focus the app on the settings page.
    OpenSettings,
    /// Focus the app on the current running mode, or home when none runs.
    OpenCurrentMode,
}

/// One row of the shortcut table.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ShortcutBinding {
    /// Display form, e.g. `Ctrl+1`.
    pub combo: &'static str,
    /// What pressing it does.
    pub action: ShortcutAction,
}

/// The full table, in registration order.
#[must_use]
pub fn bindings() -> Vec<ShortcutBinding> {
    vec![
        ShortcutBinding {
            combo: crate::stealth::shortcuts::APP_TOGGLE,
            action: ShortcutAction::ToggleApp,
        },
        ShortcutBinding {
            combo: crate::stealth::shortcuts::STEALTH_TOGGLE,
            action: ShortcutAction::ToggleUndetectable,
        },
        ShortcutBinding {
            combo: crate::stealth::shortcuts::OVERLAY_TOGGLE,
            action: ShortcutAction::ToggleOverlay,
        },
        ShortcutBinding {
            combo: crate::stealth::shortcuts::OPEN_SETTINGS,
            action: ShortcutAction::OpenSettings,
        },
        ShortcutBinding {
            combo: crate::stealth::shortcuts::OPEN_CURRENT,
            action: ShortcutAction::OpenCurrentMode,
        },
    ]
}

/// The action one combo performs, if bound.
#[must_use]
pub fn action_for(combo: &str) -> Option<ShortcutAction> {
    bindings()
        .into_iter()
        .find(|binding| binding.combo.eq_ignore_ascii_case(combo))
        .map(|binding| binding.action)
}

/// The route the main window should show for one action.
///
/// `OpenCurrentMode` resolves through the current running mode; everything
/// else is static.
#[must_use]
pub fn route_for(action: ShortcutAction, running: Option<RunningMode>) -> &'static str {
    match action {
        ShortcutAction::OpenSettings => "/settings/ai",
        ShortcutAction::OpenCurrentMode => match running {
            Some(mode) => match mode.session_id {
                Some(id) => leak_route(format!("/{}/{}", mode.mode.route_segment(), id)),
                None => mode.mode.home_route(),
            },
            None => "/",
        },
        _ => "/",
    }
}

/// Convert a dynamic route `String` to `&'static str` for the (test) return type.
///
/// Leaks intentionally: routes are few and shell-lived.
#[must_use]
pub fn leak_route(route: String) -> &'static str {
    Box::leak(route.into_boxed_str())
}

/// A session that is currently running.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RunningMode {
    /// Which product mode is running.
    pub mode: SessionMode,
    /// The running session, when the API returned one.
    pub session_id: Option<uuid::Uuid>,
}

/// Extension: the URL segment and home route for each mode.
pub trait ModeRoutes {
    /// URL segment, e.g. `live`.
    fn route_segment(&self) -> &'static str;
    /// Route of the mode's home list.
    fn home_route(&self) -> &'static str;
}

impl ModeRoutes for SessionMode {
    fn route_segment(&self) -> &'static str {
        match self {
            SessionMode::LiveInterview => "live",
            SessionMode::MockInterview => "mock",
            SessionMode::Learning => "learning",
        }
    }

    fn home_route(&self) -> &'static str {
        match self {
            SessionMode::LiveInterview => "/live",
            SessionMode::MockInterview => "/mock",
            SessionMode::Learning => "/learning",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_combo_maps_to_one_action() {
        let table = bindings();
        assert_eq!(table.len(), 5);
        for binding in &table {
            assert_eq!(action_for(binding.combo), Some(binding.action));
        }
    }

    #[test]
    fn unknown_combo_is_none() {
        assert_eq!(action_for("Ctrl+9"), None);
    }

    #[test]
    fn settings_route_is_static() {
        assert_eq!(
            route_for(ShortcutAction::OpenSettings, None),
            "/settings/ai"
        );
    }

    #[test]
    fn current_mode_opens_the_running_session() {
        let running = RunningMode {
            mode: SessionMode::LiveInterview,
            session_id: Some(uuid::Uuid::nil()),
        };
        let route = route_for(ShortcutAction::OpenCurrentMode, Some(running));
        assert!(route.starts_with("/live/"));
    }

    #[test]
    fn current_mode_without_session_opens_the_mode_home() {
        let running = RunningMode {
            mode: SessionMode::Learning,
            session_id: None,
        };
        assert_eq!(
            route_for(ShortcutAction::OpenCurrentMode, Some(running)),
            "/learning"
        );
    }

    #[test]
    fn current_mode_without_running_opens_home() {
        assert_eq!(route_for(ShortcutAction::OpenCurrentMode, None), "/");
    }
}
