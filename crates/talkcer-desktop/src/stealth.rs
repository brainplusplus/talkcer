//! Undetectable mode: keep every Talkcer window out of screen captures.
//!
//! The model follows the Natively reference (`references/natively-cluely-ai-assistant`):
//!
//! - the *decision* logic is pure and headless-testable: [`StealthState`],
//!   [`next_stealth`], [`stealth_plan`], and [`decide_toggle`] never touch a
//!   window. CI covers them without WebView2.
//! - the *application* is `WDA_EXCLUDEFROMCAPTURE` (`SetWindowDisplayAffinity`)
//!   on each of our windows on Windows. The API is silently undoable (focus
//!   changes, DWM churn), so the shell re-asserts it on a short interval while
//!   the mode is on — the same discipline the reference calls the stealth
//!   protection loop.
//! - the toggle always broadcasts the authoritative state even on a no-op, so
//!   a UI that drifted heals on the next event (the RC-2 invariant from the
//!   reference's toggleStateReducer).
//!
//! Off state is fully restored: windows go back to `WDA_NONE`.

use serde::Serialize;

/// Every global key combo the desktop shell binds.
///
/// One source of truth for the tray labels, the shortcut table, the Tauri
/// registration, and the docs. Key combos are deliberately in the `Ctrl+N`
/// family so they are usable one-handed while another window has focus.
pub mod shortcuts {
    /// Show or hide the whole desktop app (main + overlay).
    pub const APP_TOGGLE: &str = "Ctrl+1";
    /// Toggle undetectable mode.
    pub const STEALTH_TOGGLE: &str = "Ctrl+2";
    /// Toggle the overlay window.
    pub const OVERLAY_TOGGLE: &str = "Ctrl+3";
    /// Focus the desktop app on the settings page.
    pub const OPEN_SETTINGS: &str = "Ctrl+4";
    /// Focus the desktop app on the current running mode (or home).
    pub const OPEN_CURRENT: &str = "Ctrl+5";
}

/// Commands the native shell applies when undetectable mode changes.
///
/// Pure data: the Tauri layer interprets them, tests assert them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case", tag = "command")]
pub enum StealthCommand {
    /// Exclude every shell window from screen capture.
    ExcludeFromCapture,
    /// Restore capture visibility.
    RestoreCapture,
    /// Hide the shell's taskbar buttons.
    HideFromTaskbar,
    /// Restore the shell's taskbar buttons.
    ShowInTaskbar,
    /// Disguise the window titles so an enumerating proctor reads a neutral name.
    DisguiseTitles,
    /// Restore the real window titles.
    RestoreTitles,
}

/// The observable state of undetectable mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub struct StealthState {
    /// Whether windows are excluded from capture.
    pub undetectable: bool,
}

/// The decision result for one toggle request.
///
/// Mirrors the reference `toggleStateReducer`: `next` is authoritative,
/// `changed` gates the OS side-effects, `broadcast` is always true so a
/// desynced UI heals itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ToggleDecision {
    /// The authoritative next state.
    pub next: bool,
    /// Whether the value actually changed.
    pub changed: bool,
    /// Always true: reconcile observers with authoritative state.
    pub broadcast: bool,
}

/// Decide one toggle request (pure).
#[must_use]
pub fn decide_toggle(current: bool, requested: bool) -> ToggleDecision {
    ToggleDecision {
        next: requested,
        changed: current != requested,
        broadcast: true,
    }
}

/// The next state after toggling (pure).
#[must_use]
pub fn next_stealth(current: bool) -> bool {
    !current
}

/// The commands the shell must apply for one undetectable state (pure).
///
/// Turning on excludes windows from capture, hides taskbar buttons, and
/// disguises titles. Turning off restores all three. The list is complete for
/// either state so applying it is idempotent: re-running the plan on the same
/// state is a no-op at the OS level.
#[must_use]
pub fn stealth_plan(undetectable: bool) -> Vec<StealthCommand> {
    if undetectable {
        vec![
            StealthCommand::ExcludeFromCapture,
            StealthCommand::HideFromTaskbar,
            StealthCommand::DisguiseTitles,
        ]
    } else {
        vec![
            StealthCommand::RestoreCapture,
            StealthCommand::ShowInTaskbar,
            StealthCommand::RestoreTitles,
        ]
    }
}

/// The neutral title the shell uses while undetectable.
///
/// "Windows Input Experience" is a real, always-present system process name;
/// an enumerating proctor reads nothing unusual.
#[must_use]
pub fn disguise_title() -> &'static str {
    "Windows Input Experience"
}

#[cfg(all(test, windows))]
mod tests {
    use super::*;

    #[test]
    fn toggle_decision_always_broadcasts() {
        let same = decide_toggle(true, true);
        assert!(!same.changed);
        assert!(same.broadcast);
        assert!(same.next);
        let flip = decide_toggle(false, true);
        assert!(flip.changed);
        assert!(flip.broadcast);
    }

    #[test]
    fn plan_on_excludes_and_hides() {
        let plan = stealth_plan(true);
        assert!(plan.contains(&StealthCommand::ExcludeFromCapture));
        assert!(plan.contains(&StealthCommand::HideFromTaskbar));
        assert!(!plan.contains(&StealthCommand::RestoreCapture));
    }

    #[test]
    fn plan_off_restores_everything() {
        let plan = stealth_plan(false);
        assert!(plan.contains(&StealthCommand::RestoreCapture));
        assert!(plan.contains(&StealthCommand::ShowInTaskbar));
        assert!(plan.contains(&StealthCommand::RestoreTitles));
        assert!(!plan.contains(&StealthCommand::ExcludeFromCapture));
    }

    #[test]
    fn shortcuts_are_the_ctrl_number_family() {
        assert_eq!(shortcuts::APP_TOGGLE, "Ctrl+1");
        assert_eq!(shortcuts::STEALTH_TOGGLE, "Ctrl+2");
        assert_eq!(shortcuts::OVERLAY_TOGGLE, "Ctrl+3");
        assert_eq!(shortcuts::OPEN_SETTINGS, "Ctrl+4");
        assert_eq!(shortcuts::OPEN_CURRENT, "Ctrl+5");
    }
}
