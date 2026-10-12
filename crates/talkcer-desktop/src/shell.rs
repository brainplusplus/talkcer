//! Headless overlay controller.
//!
//! The controller owns the one piece of desktop behaviour that must be correct
//! on every platform: turning a user action into the API calls and window
//! commands the shell should run. It holds no window handle, so the same code
//! runs in the Tauri shell and in a test.

use std::sync::Arc;

use talkcer_core::{AssistantAction, OverlayPreferences, OverlayState, TimelineItem};
use uuid::Uuid;

use crate::client::OverlayApi;
use crate::overlay::{self, WindowCommand};
use crate::subscription::{EventOutcome, Subscription};

/// A quick action the overlay footer offers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuickAction {
    /// Explain the latest remote transcript.
    Explain,
    /// Translate the latest remote transcript.
    Translate,
    /// Suggest talking points for the latest remote transcript.
    SuggestPoints,
    /// Hide the overlay.
    Hide,
}

impl QuickAction {
    /// The assistant action this quick action requests, when it needs a model.
    #[must_use]
    pub const fn assistant_action(self) -> Option<AssistantAction> {
        match self {
            Self::Explain => Some(AssistantAction::Explain),
            Self::Translate => Some(AssistantAction::Translate),
            Self::SuggestPoints => Some(AssistantAction::SuggestPoints),
            Self::Hide => None,
        }
    }

    /// The label shown on the button.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Explain => "Explain",
            Self::Translate => "Translate",
            Self::SuggestPoints => "Suggest",
            Self::Hide => "Hide",
        }
    }
}

/// The latest remote utterance, which quick actions target.
///
/// The overlay only acts on the remote speaker so a quick action never explains
/// the user's own words back to them.
#[must_use]
pub fn latest_remote(items: &[TimelineItem]) -> Option<&TimelineItem> {
    items
        .iter()
        .rev()
        .find(|item| item.speaker == talkcer_core::Speaker::Remote && item.is_final)
}

/// What the shell should render and run after one refresh.
#[derive(Debug, Clone, PartialEq)]
pub struct ShellState {
    /// Saved preferences after the latest change.
    pub preferences: OverlayPreferences,
    /// Items the overlay may show.
    pub items: Vec<TimelineItem>,
    /// Native window commands to apply, in order.
    pub commands: Vec<WindowCommand>,
    /// Quick actions that are currently meaningful.
    pub actions: Vec<QuickAction>,
}

/// Applies user actions through the overlay API and returns window commands.
#[derive(Clone)]
pub struct OverlayController {
    api: Arc<dyn OverlayApi>,
}

impl OverlayController {
    /// Build a controller over an API port.
    #[must_use]
    pub fn new(api: Arc<dyn OverlayApi>) -> Self {
        Self { api }
    }

    /// Load preferences and the visible items, then plan the window.
    ///
    /// # Errors
    ///
    /// Returns the API error string when a request fails.
    pub async fn refresh(&self) -> Result<ShellState, String> {
        let preferences = self.api.preferences().await?;
        let subscription = Subscription::from_preferences(&preferences);
        let items = if subscription.is_visible() {
            subscription.visible_items(&self.api.view().await?)
        } else {
            Vec::new()
        };
        Ok(self.state(preferences, items))
    }

    /// Show the overlay, or hide a visible one.
    ///
    /// # Errors
    ///
    /// Returns the API error string when a request fails.
    pub async fn toggle(&self) -> Result<ShellState, String> {
        let preferences = self.api.toggle().await?;
        self.after_change(preferences).await
    }

    /// Hide the overlay from the tray or the shortcut.
    ///
    /// # Errors
    ///
    /// Returns the API error string when a request fails.
    pub async fn hide(&self) -> Result<ShellState, String> {
        let preferences = self.api.hide().await?;
        self.after_change(preferences).await
    }

    /// Move the overlay to a new position.
    ///
    /// # Errors
    ///
    /// Returns the API error string when a request fails or the overlay is hidden.
    pub async fn move_to(&self, x: i64, y: i64) -> Result<ShellState, String> {
        let preferences = self.api.move_to(x, y).await?;
        self.after_change(preferences).await
    }

    /// Change the overlay opacity.
    ///
    /// # Errors
    ///
    /// Returns the API error string when a request fails or the value is out of range.
    pub async fn set_opacity(&self, opacity: f64) -> Result<ShellState, String> {
        let preferences = self.api.set_opacity(opacity).await?;
        self.after_change(preferences).await
    }

    /// Enable or disable click-through on non-interactive space.
    ///
    /// # Errors
    ///
    /// Returns the API error string when a request fails or the overlay is hidden.
    pub async fn set_click_through(&self, enabled: bool) -> Result<ShellState, String> {
        let preferences = self.api.set_click_through(enabled).await?;
        self.after_change(preferences).await
    }

    /// Follow a session, dropping the previous session's content.
    ///
    /// # Errors
    ///
    /// Returns the API error string when a request fails.
    pub async fn follow(&self, session_id: Uuid) -> Result<ShellState, String> {
        let preferences = self.api.follow(session_id).await?;
        self.after_change(preferences).await
    }

    /// Run a quick action against the latest remote transcript.
    ///
    /// A `Hide` action never calls a model. When no remote utterance is present
    /// the request is refused locally instead of sending a bad target.
    ///
    /// # Errors
    ///
    /// Returns a message when there is no target or the API call fails.
    pub async fn quick_action(&self, action: QuickAction) -> Result<ShellState, String> {
        if action == QuickAction::Hide {
            return self.hide().await;
        }
        let state = self.refresh().await?;
        let Some(session_id) = state.preferences.active_session_id else {
            return Err("no active session to act on".to_owned());
        };
        let Some(target) = latest_remote(&state.items) else {
            return Err("no remote transcript to act on".to_owned());
        };
        let assistant = action
            .assistant_action()
            .ok_or_else(|| "action needs no model".to_owned())?;
        self.api.assistant_action(session_id, assistant, target.id).await?;
        self.refresh().await
    }

    /// Reload after one WebSocket event, when the event matters.
    ///
    /// # Errors
    ///
    /// Returns the API error string when a refresh was needed and failed.
    pub async fn on_event(
        &self,
        subscription: &Subscription,
        event: &serde_json::Value,
    ) -> Result<Option<ShellState>, String> {
        match subscription.classify(event) {
            EventOutcome::Ignore => Ok(None),
            EventOutcome::RefreshPreferences | EventOutcome::RefreshView | EventOutcome::Append => {
                self.refresh().await.map(Some)
            }
        }
    }

    async fn after_change(&self, preferences: OverlayPreferences) -> Result<ShellState, String> {
        let subscription = Subscription::from_preferences(&preferences);
        let items = if subscription.is_visible() {
            subscription.visible_items(&self.api.view().await?)
        } else {
            Vec::new()
        };
        Ok(self.state(preferences, items))
    }

    fn state(&self, preferences: OverlayPreferences, items: Vec<TimelineItem>) -> ShellState {
        let actions = if latest_remote(&items).is_some() {
            vec![
                QuickAction::Explain,
                QuickAction::Translate,
                QuickAction::SuggestPoints,
                QuickAction::Hide,
            ]
        } else {
            vec![QuickAction::Hide]
        };
        let commands = overlay::plan(&preferences);
        ShellState {
            preferences,
            items,
            commands,
            actions,
        }
    }
}

/// The commands to apply when the user closes the overlay window.
///
/// Closing the overlay must never quit the app: it hides the window and keeps
/// the workspace available.
#[must_use]
pub fn close_plan(state: OverlayState) -> Vec<WindowCommand> {
    match state {
        OverlayState::Hidden => overlay::hidden_plan(),
        _ => overlay::hidden_plan(),
    }
}
