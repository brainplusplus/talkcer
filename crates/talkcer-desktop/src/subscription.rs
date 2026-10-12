//! Active-session subscription for the overlay.
//!
//! The overlay is a subscription client, not a capture engine. It shows exactly
//! one session: the one saved as `active_session_id`. This module decides which
//! incoming WebSocket events and timeline items belong to that session, so a
//! session switch never leaks another session's transcript.

use talkcer_core::{OverlayPreferences, OverlayState, TimelineItem};
use uuid::Uuid;

/// What the shell should do after one incoming WebSocket event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EventOutcome {
    /// The event is unrelated to the overlay; ignore it.
    Ignore,
    /// Reload preferences, because the overlay state changed.
    RefreshPreferences,
    /// Reload the visible timeline, because the active session gained content.
    RefreshView,
    /// Append one item without a round trip. Only used for the active session.
    Append,
}

/// The overlay subscription state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscription {
    active_session: Option<Uuid>,
    visible: bool,
}

impl Subscription {
    /// Build a subscription from saved preferences.
    #[must_use]
    pub fn from_preferences(preferences: &OverlayPreferences) -> Self {
        Self {
            active_session: preferences.active_session_id,
            visible: preferences.state != OverlayState::Hidden,
        }
    }

    /// The session the overlay follows, if any.
    #[must_use]
    pub fn active_session(&self) -> Option<Uuid> {
        self.active_session
    }

    /// Whether the overlay is currently allowed to show content.
    #[must_use]
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Follow a different session. Content from the previous session is dropped.
    pub fn switch(&mut self, session_id: Uuid) {
        self.active_session = Some(session_id);
    }

    /// Whether an item from `session_id` may be shown.
    #[must_use]
    pub fn accepts(&self, session_id: Uuid) -> bool {
        self.visible && self.active_session == Some(session_id)
    }

    /// Filter a batch of items to the ones the overlay may show.
    #[must_use]
    pub fn visible_items(&self, items: &[TimelineItem]) -> Vec<TimelineItem> {
        items
            .iter()
            .filter(|item| self.accepts(item.session_id))
            .cloned()
            .collect()
    }

    /// Classify one incoming WebSocket event.
    ///
    /// Events without a `session_id` are ignored except
    /// `overlay.state_changed`, which always forces a preference reload.
    #[must_use]
    pub fn classify(&self, event: &serde_json::Value) -> EventOutcome {
        let kind = event.get("type").and_then(serde_json::Value::as_str).unwrap_or("");
        if kind == "overlay.state_changed" || kind == "overlay.update_preferences" {
            return EventOutcome::RefreshPreferences;
        }
        let Some(raw) = event.get("session_id").and_then(serde_json::Value::as_str) else {
            return EventOutcome::Ignore;
        };
        let Ok(session_id) = Uuid::parse_str(raw) else {
            return EventOutcome::Ignore;
        };
        if !self.accepts(session_id) {
            return EventOutcome::Ignore;
        }
        match kind {
            "transcript.updated" | "assistant.delta" | "assistant.completed" => EventOutcome::RefreshView,
            _ => EventOutcome::Ignore,
        }
    }
}
