//! Overlay API client.
//!
//! The desktop shell is a client of the core API, never a capture engine. This
//! module defines the small port the shell needs and one HTTP implementation.
//! Tests substitute a fake so the shell logic runs without a server.

use async_trait::async_trait;
use talkcer_core::{AssistantAction, OverlayPreferences, TimelineItem};
use uuid::Uuid;

/// The overlay operations the desktop shell performs against the core API.
///
/// Every method mirrors one HTTP route. Errors are strings because the shell
/// only shows them; the API already returns a typed message.
#[async_trait]
pub trait OverlayApi: Send + Sync {
    /// Load the saved preferences.
    async fn preferences(&self) -> Result<OverlayPreferences, String>;
    /// Load the timeline items the active session may show.
    async fn view(&self) -> Result<Vec<TimelineItem>, String>;
    /// Show the overlay above other windows.
    async fn show(&self) -> Result<OverlayPreferences, String>;
    /// Hide the overlay completely.
    async fn hide(&self) -> Result<OverlayPreferences, String>;
    /// Show a hidden overlay, or hide a visible one.
    async fn toggle(&self) -> Result<OverlayPreferences, String>;
    /// Move a visible overlay.
    async fn move_to(&self, x: i64, y: i64) -> Result<OverlayPreferences, String>;
    /// Change the opacity of a visible overlay.
    async fn set_opacity(&self, opacity: f64) -> Result<OverlayPreferences, String>;
    /// Ignore pointer events on non-interactive overlay space.
    async fn set_click_through(&self, enabled: bool) -> Result<OverlayPreferences, String>;
    /// Exclude every shell window from screen capture, or restore them.
    async fn set_undetectable(&self, enabled: bool) -> Result<OverlayPreferences, String>;
    /// Follow one session.
    async fn follow(&self, session_id: Uuid) -> Result<OverlayPreferences, String>;
    /// Ask the session's thinker for one assistant action.
    async fn assistant_action(
        &self,
        session_id: Uuid,
        action: AssistantAction,
        target_id: Uuid,
    ) -> Result<(), String>;
}

/// Overlay API over the local core server.
#[derive(Debug, Clone)]
pub struct HttpOverlayApi {
    base: String,
    client: reqwest::Client,
}

impl HttpOverlayApi {
    /// Build a client for one API base such as `http://127.0.0.1:54000/api`.
    #[must_use]
    pub fn new(base: impl Into<String>) -> Self {
        Self {
            base: base.into().trim_end_matches('/').to_owned(),
            client: reqwest::Client::new(),
        }
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base, path)
    }

    async fn preferences_call(&self, method: reqwest::Method, path: &str) -> Result<OverlayPreferences, String> {
        let response = self
            .client
            .request(method, self.url(path))
            .send()
            .await
            .map_err(|error| format!("overlay request failed: {error}"))?;
        decode(response).await
    }

    async fn preferences_body(
        &self,
        path: &str,
        body: serde_json::Value,
    ) -> Result<OverlayPreferences, String> {
        let response = self
            .client
            .post(self.url(path))
            .json(&body)
            .send()
            .await
            .map_err(|error| format!("overlay request failed: {error}"))?;
        decode(response).await
    }
}

async fn decode<T: serde::de::DeserializeOwned>(response: reqwest::Response) -> Result<T, String> {
    let status = response.status();
    if !status.is_success() {
        let detail = response.text().await.unwrap_or_default();
        return Err(format!("overlay request returned {status}: {}", detail.trim()));
    }
    response
        .json::<T>()
        .await
        .map_err(|error| format!("overlay response was not JSON: {error}"))
}

#[async_trait]
impl OverlayApi for HttpOverlayApi {
    async fn preferences(&self) -> Result<OverlayPreferences, String> {
        self.preferences_call(reqwest::Method::GET, "/overlay").await
    }

    async fn view(&self) -> Result<Vec<TimelineItem>, String> {
        let response = self
            .client
            .get(self.url("/overlay/view"))
            .send()
            .await
            .map_err(|error| format!("overlay request failed: {error}"))?;
        let view: ViewBody = decode(response).await?;
        Ok(view.items)
    }

    async fn show(&self) -> Result<OverlayPreferences, String> {
        self.preferences_call(reqwest::Method::POST, "/overlay/show").await
    }

    async fn hide(&self) -> Result<OverlayPreferences, String> {
        self.preferences_call(reqwest::Method::POST, "/overlay/hide").await
    }

    async fn toggle(&self) -> Result<OverlayPreferences, String> {
        self.preferences_call(reqwest::Method::POST, "/overlay/toggle").await
    }

    async fn move_to(&self, x: i64, y: i64) -> Result<OverlayPreferences, String> {
        self.preferences_body("/overlay/move", serde_json::json!({ "x": x, "y": y }))
            .await
    }

    async fn set_opacity(&self, opacity: f64) -> Result<OverlayPreferences, String> {
        self.preferences_body("/overlay/opacity", serde_json::json!({ "opacity": opacity }))
            .await
    }

    async fn set_click_through(&self, enabled: bool) -> Result<OverlayPreferences, String> {
        self.preferences_body("/overlay/click-through", serde_json::json!({ "enabled": enabled }))
            .await
    }

    async fn set_undetectable(&self, enabled: bool) -> Result<OverlayPreferences, String> {
        self.preferences_body("/overlay/undetectable", serde_json::json!({ "enabled": enabled }))
            .await
    }

    async fn follow(&self, session_id: Uuid) -> Result<OverlayPreferences, String> {
        self.preferences_body("/overlay", serde_json::json!({ "session_id": session_id }))
            .await
    }

    async fn assistant_action(
        &self,
        session_id: Uuid,
        action: AssistantAction,
        target_id: Uuid,
    ) -> Result<(), String> {
        let response = self
            .client
            .post(self.url(&format!("/sessions/{session_id}/assistant-actions")))
            .json(&serde_json::json!({
                "action": action,
                "target": { "kind": "utterance", "id": target_id },
            }))
            .send()
            .await
            .map_err(|error| format!("assistant action failed: {error}"))?;
        let status = response.status();
        if !status.is_success() {
            let detail = response.text().await.unwrap_or_default();
            return Err(format!("assistant action returned {status}: {}", detail.trim()));
        }
        Ok(())
    }
}

#[derive(serde::Deserialize)]
struct ViewBody {
    items: Vec<TimelineItem>,
}
