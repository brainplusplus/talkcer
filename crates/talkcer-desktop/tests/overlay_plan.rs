//! Headless checks for the native overlay window plan.

use talkcer_core::{OverlayPreferences, OverlayState};

fn preferences(state: OverlayState) -> OverlayPreferences {
    OverlayPreferences {
        state,
        always_on_top: state != OverlayState::Hidden,
        opacity: 0.8,
        width: 420,
        height: 240,
        position_x: 12,
        position_y: 18,
        active_session_id: None,
        show_remote_transcript: true,
        show_local_transcript: true,
        show_ai_response: true,
        updated_at: chrono::Utc::now(),
    }
}

#[test]
fn hidden_overlay_has_no_window() {
    assert_eq!(talkcer_desktop::overlay::plan(&preferences(OverlayState::Hidden)), vec![talkcer_desktop::overlay::WindowCommand::Hide]);
}

#[test]
fn visible_overlay_stays_on_top_and_click_through_ignores_the_pointer() {
    let shown = talkcer_desktop::overlay::plan(&preferences(OverlayState::Visible));
    assert!(matches!(shown[0], talkcer_desktop::overlay::WindowCommand::Show { always_on_top: true, .. }));
    let clicked = talkcer_desktop::overlay::plan(&preferences(OverlayState::ClickThrough));
    assert!(clicked.contains(&talkcer_desktop::overlay::WindowCommand::IgnoreCursor(true)));
}

#[test]
fn shortcut_matches_the_overlay_toggle() {
    assert_eq!(talkcer_desktop::overlay::OVERLAY_SHORTCUT, "Ctrl+Shift+Space");
}
