//! Tray menu and global shortcut model.
//!
//! The Tauri shell builds its tray menu and registers the global shortcut from
//! these definitions, so the menu items and their behaviour are testable
//! without a native window.

use crate::shell::QuickAction;

/// One tray menu entry.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TrayItem {
    /// Toggle the overlay visibility.
    ToggleOverlay,
    /// Show the overlay.
    ShowOverlay,
    /// Hide the overlay.
    HideOverlay,
    /// Open the main workspace window.
    OpenWorkspace,
    /// A separator between groups.
    Separator,
    /// Quit the whole application.
    Quit,
}

/// The tray menu, in display order.
#[must_use]
pub fn tray_menu() -> Vec<TrayItem> {
    vec![
        TrayItem::ToggleOverlay,
        TrayItem::ShowOverlay,
        TrayItem::HideOverlay,
        TrayItem::Separator,
        TrayItem::OpenWorkspace,
        TrayItem::Separator,
        TrayItem::Quit,
    ]
}

/// The label for one tray entry. Separators have no label.
#[must_use]
pub fn tray_label(item: &TrayItem) -> &'static str {
    match item {
        TrayItem::ToggleOverlay => "Toggle overlay",
        TrayItem::ShowOverlay => "Show overlay",
        TrayItem::HideOverlay => "Hide overlay",
        TrayItem::OpenWorkspace => "Open workspace",
        TrayItem::Separator => "",
        TrayItem::Quit => "Quit",
    }
}

/// The overlay footer buttons, in display order.
#[must_use]
pub fn quick_actions() -> Vec<QuickAction> {
    vec![
        QuickAction::Explain,
        QuickAction::Translate,
        QuickAction::SuggestPoints,
        QuickAction::Hide,
    ]
}

/// The global shortcut that toggles the overlay while another window has focus.
pub const GLOBAL_SHORTCUT: &str = crate::stealth::shortcuts::OVERLAY_TOGGLE;

/// The default workspace URL the desktop shell opens.
pub const WORKSPACE_URL: &str = "http://127.0.0.1:54000/";
