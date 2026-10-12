//! Headless desktop shell behaviour.
//!
//! A fake API stands in for the core server so the overlay controller, the
//! subscription filter, and the tray model are verified without a window or a
//! running server.

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use talkcer_core::{
    AssistantAction, ItemKind, OverlayPreferences, OverlayState, Purpose, Source, Speaker,
    TimelineItem,
};
use talkcer_desktop::client::OverlayApi;
use talkcer_desktop::overlay::{self, WindowCommand};
use talkcer_desktop::shell::{self, OverlayController, QuickAction};
use talkcer_desktop::subscription::{EventOutcome, Subscription};
use talkcer_desktop::tray::{self, TrayItem};
use uuid::Uuid;

/// A recording fake of the overlay API.
#[derive(Clone)]
struct FakeApi {
    state: Arc<Mutex<OverlayPreferences>>,
    items: Arc<Mutex<Vec<TimelineItem>>>,
    calls: Arc<Mutex<Vec<String>>>,
}

impl FakeApi {
    fn new(active: Option<Uuid>) -> Self {
        let mut preferences = preferences(OverlayState::Hidden);
        preferences.active_session_id = active;
        Self {
            state: Arc::new(Mutex::new(preferences)),
            items: Arc::new(Mutex::new(Vec::new())),
            calls: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn with_items(self, items: Vec<TimelineItem>) -> Self {
        *self.items.lock().unwrap() = items;
        self
    }

    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }

    fn current(&self) -> OverlayPreferences {
        self.state.lock().unwrap().clone()
    }

    fn record(&self, call: &str) {
        self.calls.lock().unwrap().push(call.to_owned());
    }
}

#[async_trait]
impl OverlayApi for FakeApi {
    async fn preferences(&self) -> Result<OverlayPreferences, String> {
        self.record("preferences");
        Ok(self.current())
    }

    async fn view(&self) -> Result<Vec<TimelineItem>, String> {
        self.record("view");
        Ok(self.items.lock().unwrap().clone())
    }

    async fn show(&self) -> Result<OverlayPreferences, String> {
        self.record("show");
        self.state.lock().unwrap().state = OverlayState::Visible;
        self.state.lock().unwrap().always_on_top = true;
        Ok(self.current())
    }

    async fn hide(&self) -> Result<OverlayPreferences, String> {
        self.record("hide");
        self.state.lock().unwrap().state = OverlayState::Hidden;
        self.state.lock().unwrap().always_on_top = false;
        Ok(self.current())
    }

    async fn toggle(&self) -> Result<OverlayPreferences, String> {
        self.record("toggle");
        if self.current().state == OverlayState::Hidden {
            self.show().await
        } else {
            self.hide().await
        }
    }

    async fn move_to(&self, x: i64, y: i64) -> Result<OverlayPreferences, String> {
        self.record("move");
        {
            let mut state = self.state.lock().unwrap();
            state.position_x = x;
            state.position_y = y;
        }
        Ok(self.current())
    }

    async fn set_opacity(&self, opacity: f64) -> Result<OverlayPreferences, String> {
        self.record("opacity");
        if !(0.2..=1.0).contains(&opacity) {
            return Err("opacity must be between 0.2 and 1".to_owned());
        }
        self.state.lock().unwrap().opacity = opacity;
        Ok(self.current())
    }

    async fn set_click_through(&self, enabled: bool) -> Result<OverlayPreferences, String> {
        self.record("click_through");
        self.state.lock().unwrap().state = if enabled {
            OverlayState::ClickThrough
        } else {
            OverlayState::Visible
        };
        Ok(self.current())
    }

    async fn set_undetectable(&self, enabled: bool) -> Result<OverlayPreferences, String> {
        self.record("undetectable");
        self.state.lock().unwrap().undetectable = enabled;
        Ok(self.current())
    }

    async fn follow(&self, session_id: Uuid) -> Result<OverlayPreferences, String> {
        self.record("follow");
        self.state.lock().unwrap().active_session_id = Some(session_id);
        Ok(self.current())
    }

    async fn assistant_action(
        &self,
        session_id: Uuid,
        action: AssistantAction,
        target_id: Uuid,
    ) -> Result<(), String> {
        self.record(&format!("assistant:{}:{}:{}", session_id, action.as_str(), target_id));
        Ok(())
    }
}

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
        undetectable: false,
        updated_at: chrono::Utc::now(),
    }
}

fn item(session: Uuid, speaker: Speaker, text: &str, is_final: bool) -> TimelineItem {
    TimelineItem {
        id: Uuid::now_v7(),
        session_id: session,
        kind: ItemKind::Transcript,
        speaker,
        source: Source::Text,
        purpose: Purpose::Conversation,
        text: text.to_owned(),
        started_at_ms: 1_000,
        ended_at_ms: None,
        is_final,
        metadata: serde_json::json!({}),
        created_at: chrono::Utc::now(),
        utterance_id: None,
    }
}

fn controller(api: &FakeApi) -> OverlayController {
    OverlayController::new(Arc::new(api.clone()))
}

#[test]
fn hidden_overlay_has_no_window() {
    assert_eq!(
        overlay::plan(&preferences(OverlayState::Hidden)),
        vec![WindowCommand::Hide]
    );
}

#[test]
fn visible_overlay_stays_on_top_and_click_through_ignores_the_pointer() {
    let shown = overlay::plan(&preferences(OverlayState::Visible));
    assert!(matches!(
        shown[0],
        WindowCommand::Show { always_on_top: true, .. }
    ));
    let clicked = overlay::plan(&preferences(OverlayState::ClickThrough));
    assert!(clicked.contains(&WindowCommand::IgnoreCursor(true)));
}

#[test]
fn shortcut_matches_the_overlay_toggle() {
    assert_eq!(overlay::OVERLAY_SHORTCUT, "Ctrl+3");
    assert_eq!(tray::GLOBAL_SHORTCUT, overlay::OVERLAY_SHORTCUT);
}

#[test]
fn overlay_state_machine_matches_the_docs() {
    assert!(overlay::can_transition(OverlayState::Hidden, OverlayState::Visible));
    assert!(overlay::can_transition(OverlayState::Visible, OverlayState::ClickThrough));
    assert!(overlay::can_transition(OverlayState::ClickThrough, OverlayState::Visible));
    assert!(overlay::can_transition(OverlayState::Visible, OverlayState::Hidden));
    // A hidden overlay never jumps straight to click-through.
    assert!(!overlay::can_transition(OverlayState::Hidden, OverlayState::ClickThrough));
}

#[test]
fn closing_the_overlay_hides_it_without_quitting() {
    assert_eq!(shell::close_plan(OverlayState::Visible), vec![WindowCommand::Hide]);
    assert_eq!(shell::close_plan(OverlayState::ClickThrough), vec![WindowCommand::Hide]);
}

#[test]
fn tray_menu_covers_overlay_lifecycle_and_workspace() {
    let menu = tray::tray_menu();
    assert_eq!(menu.first(), Some(&TrayItem::ToggleOverlay));
    assert!(menu.contains(&TrayItem::ShowOverlay));
    assert!(menu.contains(&TrayItem::HideOverlay));
    assert!(menu.contains(&TrayItem::OpenWorkspace));
    assert_eq!(menu.last(), Some(&TrayItem::Quit));
    assert_eq!(tray::tray_label(&TrayItem::OpenWorkspace), "Open workspace");
    assert_eq!(tray::WORKSPACE_URL, "http://127.0.0.1:54000/");
}

#[test]
fn subscription_only_accepts_the_active_session() {
    let first = Uuid::now_v7();
    let second = Uuid::now_v7();
    let mut prefs = preferences(OverlayState::Visible);
    prefs.active_session_id = Some(first);
    let subscription = Subscription::from_preferences(&prefs);
    assert!(subscription.accepts(first));
    assert!(!subscription.accepts(second));

    let items = vec![
        item(first, Speaker::Remote, "keep", true),
        item(second, Speaker::Ai, "drop", true),
    ];
    let visible = subscription.visible_items(&items);
    assert_eq!(visible.len(), 1);
    assert_eq!(visible[0].text, "keep");
}

#[test]
fn subscription_ignores_other_sessions_and_classifies_overlay_events() {
    let active = Uuid::now_v7();
    let other = Uuid::now_v7();
    let mut prefs = preferences(OverlayState::Visible);
    prefs.active_session_id = Some(active);
    let subscription = Subscription::from_preferences(&prefs);

    assert_eq!(
        subscription.classify(&serde_json::json!({
            "type": "transcript.updated",
            "session_id": active,
        })),
        EventOutcome::RefreshView
    );
    assert_eq!(
        subscription.classify(&serde_json::json!({
            "type": "assistant.completed",
            "session_id": active,
        })),
        EventOutcome::RefreshView
    );
    assert_eq!(
        subscription.classify(&serde_json::json!({
            "type": "transcript.updated",
            "session_id": other,
        })),
        EventOutcome::Ignore
    );
    assert_eq!(
        subscription.classify(&serde_json::json!({ "type": "overlay.state_changed", "visible": true })),
        EventOutcome::RefreshPreferences
    );
    assert_eq!(
        subscription.classify(&serde_json::json!({ "type": "capture.state_changed", "session_id": active })),
        EventOutcome::Ignore
    );
}

#[test]
fn hidden_subscription_accepts_nothing() {
    let active = Uuid::now_v7();
    let mut prefs = preferences(OverlayState::Hidden);
    prefs.active_session_id = Some(active);
    let subscription = Subscription::from_preferences(&prefs);
    assert!(!subscription.accepts(active));
    assert_eq!(
        subscription.classify(&serde_json::json!({ "type": "transcript.updated", "session_id": active })),
        EventOutcome::Ignore
    );
}

#[tokio::test]
async fn hidden_overlay_loads_no_items() {
    let session = Uuid::now_v7();
    let api = FakeApi::new(Some(session))
        .with_items(vec![item(session, Speaker::Remote, "secret", true)]);
    let state = controller(&api).refresh().await.unwrap();
    assert_eq!(state.preferences.state, OverlayState::Hidden);
    assert!(state.items.is_empty());
    assert_eq!(state.commands, vec![WindowCommand::Hide]);
    // A hidden overlay must not even read the timeline.
    assert!(!api.calls().contains(&"view".to_owned()));
}

#[tokio::test]
async fn show_move_opacity_and_hide_update_the_window_plan() {
    let session = Uuid::now_v7();
    let api = FakeApi::new(Some(session));
    let controller = controller(&api);

    let shown = controller.toggle().await.unwrap();
    assert_eq!(shown.preferences.state, OverlayState::Visible);
    assert!(matches!(
        shown.commands[0],
        WindowCommand::Show { always_on_top: true, .. }
    ));

    let moved = controller.move_to(300, 400).await.unwrap();
    assert!(matches!(
        moved.commands[0],
        WindowCommand::Show { x: 300, y: 400, .. }
    ));

    let dimmed = controller.set_opacity(0.4).await.unwrap();
    assert!((dimmed.preferences.opacity - 0.4).abs() < f64::EPSILON);
    assert!(matches!(dimmed.commands[0], WindowCommand::Show { opacity, .. } if (opacity - 0.4).abs() < f64::EPSILON));

    let hidden = controller.hide().await.unwrap();
    assert_eq!(hidden.preferences.state, OverlayState::Hidden);
    assert_eq!(hidden.commands, vec![WindowCommand::Hide]);
}

#[tokio::test]
async fn out_of_range_opacity_is_rejected() {
    let api = FakeApi::new(None);
    let error = controller(&api).set_opacity(1.5).await.unwrap_err();
    assert!(error.contains("0.2 and 1"), "{error}");
}

#[tokio::test]
async fn session_switch_drops_the_previous_session_items() {
    let first = Uuid::now_v7();
    let second = Uuid::now_v7();
    let api = FakeApi::new(Some(first)).with_items(vec![
        item(first, Speaker::Remote, "from a", true),
        item(second, Speaker::Ai, "from b", true),
    ]);
    let controller = controller(&api);

    let shown = controller.toggle().await.unwrap();
    assert_eq!(shown.items.len(), 1);
    assert_eq!(shown.items[0].text, "from a");

    let switched = controller.follow(second).await.unwrap();
    assert_eq!(switched.preferences.active_session_id, Some(second));
    assert_eq!(switched.items.len(), 1);
    assert_eq!(switched.items[0].text, "from b");
}

#[tokio::test]
async fn quick_action_targets_the_latest_remote_utterance() {
    let session = Uuid::now_v7();
    let remote = item(session, Speaker::Remote, "How would you handle retries?", true);
    let local = item(session, Speaker::LocalUser, "I would use idempotency keys.", true);
    let api = FakeApi::new(Some(session)).with_items(vec![remote.clone(), local]);
    let controller = controller(&api);
    controller.toggle().await.unwrap();

    let state = controller.quick_action(QuickAction::Explain).await.unwrap();
    let calls = api.calls();
    let action = calls
        .iter()
        .find(|call| call.starts_with("assistant:"))
        .expect("an assistant action was requested");
    assert!(action.contains("explain"), "{action}");
    assert!(action.contains(&remote.id.to_string()), "{action}");
    // The overlay footer offers every action when a remote utterance exists.
    assert_eq!(
        state.actions,
        vec![QuickAction::Explain, QuickAction::Translate, QuickAction::SuggestPoints, QuickAction::Hide]
    );
}

#[tokio::test]
async fn quick_action_refuses_without_a_remote_target() {
    let session = Uuid::now_v7();
    let api = FakeApi::new(Some(session))
        .with_items(vec![item(session, Speaker::LocalUser, "only me", true)]);
    let controller = controller(&api);
    controller.toggle().await.unwrap();

    let error = controller.quick_action(QuickAction::Translate).await.unwrap_err();
    assert!(error.contains("no remote transcript"), "{error}");
    assert!(!api.calls().iter().any(|call| call.starts_with("assistant:")));
}

#[tokio::test]
async fn hide_quick_action_never_calls_the_model() {
    let session = Uuid::now_v7();
    let api = FakeApi::new(Some(session));
    let controller = controller(&api);
    controller.toggle().await.unwrap();

    let state = controller.quick_action(QuickAction::Hide).await.unwrap();
    assert_eq!(state.preferences.state, OverlayState::Hidden);
    assert_eq!(state.commands, vec![WindowCommand::Hide]);
    assert!(!api.calls().iter().any(|call| call.starts_with("assistant:")));
}

#[tokio::test]
async fn subscription_event_triggers_a_refresh_only_when_relevant() {
    let active = Uuid::now_v7();
    let other = Uuid::now_v7();
    let mut prefs = preferences(OverlayState::Visible);
    prefs.active_session_id = Some(active);
    let subscription = Subscription::from_preferences(&prefs);
    let api = FakeApi::new(Some(active));
    let controller = controller(&api);

    let ignored = controller
        .on_event(&subscription, &serde_json::json!({ "type": "transcript.updated", "session_id": other }))
        .await
        .unwrap();
    assert!(ignored.is_none());
    assert!(api.calls().is_empty());

    let refreshed = controller
        .on_event(&subscription, &serde_json::json!({ "type": "transcript.updated", "session_id": active }))
        .await
        .unwrap();
    assert!(refreshed.is_some());
    assert!(api.calls().contains(&"preferences".to_owned()));
}

#[test]
fn quick_action_labels_and_actions_match_the_ui_spec() {
    assert_eq!(tray::quick_actions().len(), 4);
    assert_eq!(QuickAction::Explain.label(), "Explain");
    assert_eq!(QuickAction::SuggestPoints.assistant_action(), Some(AssistantAction::SuggestPoints));
    assert_eq!(QuickAction::Hide.assistant_action(), None);
}

#[test]
fn desktop_points_at_the_local_api() {
    assert_eq!(talkcer_desktop::VERSION, "0.1.0");
    assert_eq!(talkcer_desktop::server_bind(), "127.0.0.1:54000");
    assert_eq!(talkcer_desktop::DEFAULT_API, "http://127.0.0.1:54000/api");
}
