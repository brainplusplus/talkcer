//! Overlay preferences and the session shown in the overlay.
//!
//! Window flags are stored here. Opening the native window is a desktop
//! concern and is covered by headless command planning.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::error::CoreError;
use crate::timeline::{Speaker, TimelineItem, TimelineService};

const ROW_ID: &str = "default";

/// Whether the overlay window is shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OverlayState {
    /// The window is closed.
    Hidden,
    /// The window is visible and can be dragged.
    Visible,
    /// The window is visible, but empty space ignores the pointer.
    ClickThrough,
}

/// Saved overlay window and content flags.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OverlayPreferences {
    /// Visibility state.
    pub state: OverlayState,
    /// Native windows should stay above other windows while visible.
    pub always_on_top: bool,
    /// Opacity from 0.2 through 1.
    pub opacity: f64,
    /// Window width in pixels.
    pub width: i64,
    /// Window height in pixels.
    pub height: i64,
    /// Horizontal position.
    pub position_x: i64,
    /// Vertical position.
    pub position_y: i64,
    /// Which session the overlay follows.
    pub active_session_id: Option<Uuid>,
    /// Include the remote speaker.
    pub show_remote_transcript: bool,
    /// Include the local speaker.
    pub show_local_transcript: bool,
    /// Include assistant responses.
    pub show_ai_response: bool,
    /// Keep every shell window out of screen captures.
    pub undetectable: bool,
    /// Last change.
    pub updated_at: DateTime<Utc>,
}

/// Timeline content the overlay is allowed to show.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct OverlayView {
    /// Preferences after the latest change.
    pub preferences: OverlayPreferences,
    /// Items from the active session only.
    pub items: Vec<TimelineItem>,
}

/// A pointer position for the overlay window.
#[derive(Debug, Clone, Deserialize)]
pub struct OverlayPosition {
    /// Horizontal position.
    pub x: i64,
    /// Vertical position.
    pub y: i64,
}

/// Loads and stores the single overlay preference row.
#[derive(Clone)]
pub struct OverlayService<T> {
    pool: SqlitePool,
    timeline: TimelineService<T>,
}

impl<T> OverlayService<T>
where
    T: crate::timeline::TimelineRepository,
{
    /// Create a service over the shared pool.
    #[must_use]
    pub fn new(pool: SqlitePool, timeline: TimelineService<T>) -> Self {
        Self { pool, timeline }
    }

    /// Return the saved preferences, creating the default row when needed.
    ///
    /// # Errors
    ///
    /// Returns a database error when the row cannot be read.
    pub async fn get(&self) -> Result<OverlayPreferences, CoreError> {
        self.ensure().await?;
        self.load().await
    }

    /// Show the overlay above other windows.
    ///
    /// # Errors
    ///
    /// Returns a database error when the row cannot be saved.
    pub async fn show(&self) -> Result<OverlayPreferences, CoreError> {
        self.ensure().await?;
        sqlx::query(
            "UPDATE overlay_preferences
             SET enabled = 1, always_on_top = 1, updated_at = ?2
             WHERE id = ?1",
        )
        .bind(ROW_ID)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        self.load().await
    }

    /// Hide the overlay completely.
    ///
    /// # Errors
    ///
    /// Returns a database error when the row cannot be saved.
    pub async fn hide(&self) -> Result<OverlayPreferences, CoreError> {
        self.ensure().await?;
        sqlx::query(
            "UPDATE overlay_preferences
             SET enabled = 0, always_on_top = 0, click_through_non_interactive_area = 0, updated_at = ?2
             WHERE id = ?1",
        )
        .bind(ROW_ID)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        self.load().await
    }

    /// Show a hidden overlay, or hide a visible one.
    ///
    /// # Errors
    ///
    /// Returns a database error when the row cannot be saved.
    pub async fn toggle(&self) -> Result<OverlayPreferences, CoreError> {
        if self.get().await?.state == OverlayState::Hidden {
            self.show().await
        } else {
            self.hide().await
        }
    }

    /// Move a visible overlay.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidOverlay`] when the overlay is hidden.
    pub async fn move_to(
        &self,
        position: OverlayPosition,
    ) -> Result<OverlayPreferences, CoreError> {
        self.require_visible().await?;
        sqlx::query(
            "UPDATE overlay_preferences SET position_x = ?2, position_y = ?3, updated_at = ?4 WHERE id = ?1",
        )
        .bind(ROW_ID)
        .bind(position.x)
        .bind(position.y)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        self.load().await
    }

    /// Change opacity of a visible overlay.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidOverlay`] when the value is outside 0.2..=1 or the overlay is hidden.
    pub async fn set_opacity(&self, opacity: f64) -> Result<OverlayPreferences, CoreError> {
        if !(0.2..=1.0).contains(&opacity) {
            return Err(CoreError::InvalidOverlay(
                "opacity must be between 0.2 and 1".to_owned(),
            ));
        }
        self.require_visible().await?;
        sqlx::query("UPDATE overlay_preferences SET opacity = ?2, updated_at = ?3 WHERE id = ?1")
            .bind(ROW_ID)
            .bind(opacity)
            .bind(Utc::now().to_rfc3339())
            .execute(&self.pool)
            .await?;
        self.load().await
    }

    /// Ignore pointer events on non-interactive overlay space.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidOverlay`] when the overlay is hidden.
    pub async fn set_click_through(&self, enabled: bool) -> Result<OverlayPreferences, CoreError> {
        self.require_visible().await?;
        sqlx::query(
            "UPDATE overlay_preferences
             SET click_through_non_interactive_area = ?2, updated_at = ?3
             WHERE id = ?1",
        )
        .bind(ROW_ID)
        .bind(i64::from(enabled))
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        self.load().await
    }

    /// Turn undetectable mode on or off.
    ///
    /// This is app-wide: while on, the desktop shell excludes every window from
    /// screen capture, hides the taskbar button, and disguises the window title.
    ///
    /// # Errors
    ///
    /// Returns a database error when the row cannot be saved.
    pub async fn set_undetectable(&self, enabled: bool) -> Result<OverlayPreferences, CoreError> {
        self.ensure().await?;
        sqlx::query(
            "UPDATE overlay_preferences SET undetectable = ?2, updated_at = ?3 WHERE id = ?1",
        )
        .bind(ROW_ID)
        .bind(i64::from(enabled))
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        self.load().await
    }

    /// Follow one session. The previous session disappears from the view.
    ///
    /// # Errors
    ///
    /// Returns a database error when the row cannot be saved.
    pub async fn follow(&self, session_id: Uuid) -> Result<OverlayPreferences, CoreError> {
        self.ensure().await?;
        sqlx::query(
            "UPDATE overlay_preferences SET active_session_id = ?2, updated_at = ?3 WHERE id = ?1",
        )
        .bind(ROW_ID)
        .bind(session_id.to_string())
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        self.load().await
    }

    /// Return only the active session content allowed by the display flags.
    ///
    /// A hidden overlay returns no items. A response that belongs to another
    /// session is never included.
    ///
    /// # Errors
    ///
    /// Returns a database error when preferences or timeline items cannot be read.
    pub async fn view(&self) -> Result<OverlayView, CoreError> {
        let preferences = self.get().await?;
        if preferences.state == OverlayState::Hidden {
            return Ok(OverlayView {
                preferences,
                items: Vec::new(),
            });
        }
        let Some(session_id) = preferences.active_session_id else {
            return Ok(OverlayView {
                preferences,
                items: Vec::new(),
            });
        };
        let page = self
            .timeline
            .list(
                session_id,
                crate::timeline::TimelineQuery {
                    limit: 200,
                    before: None,
                },
            )
            .await?;
        let items = page
            .items
            .into_iter()
            .filter(|item| visible_speaker(&preferences, item.speaker))
            .collect();
        Ok(OverlayView { preferences, items })
    }

    async fn require_visible(&self) -> Result<(), CoreError> {
        if self.get().await?.state == OverlayState::Hidden {
            return Err(CoreError::InvalidOverlay(
                "show the overlay before changing it".to_owned(),
            ));
        }
        Ok(())
    }

    async fn ensure(&self) -> Result<(), CoreError> {
        let exists: Option<String> =
            sqlx::query_scalar("SELECT id FROM overlay_preferences WHERE id = ?1")
                .bind(ROW_ID)
                .fetch_optional(&self.pool)
                .await?;
        if exists.is_some() {
            return Ok(());
        }
        sqlx::query(
            "INSERT INTO overlay_preferences (
                id, enabled, always_on_top, opacity, width, height, position_x, position_y,
                click_through_non_interactive_area, show_remote_transcript, show_local_transcript,
                show_ai_response, active_session_id, undetectable, updated_at
            ) VALUES (?1, 0, 0, 0.95, 420, 320, 40, 40, 0, 1, 1, 1, NULL, 0, ?2)",
        )
        .bind(ROW_ID)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn load(&self) -> Result<OverlayPreferences, CoreError> {
        let row = sqlx::query(
            "SELECT enabled, always_on_top, opacity, width, height, position_x, position_y,
                    click_through_non_interactive_area, show_remote_transcript, show_local_transcript,
                    show_ai_response, active_session_id, undetectable, updated_at
             FROM overlay_preferences WHERE id = ?1",
        )
        .bind(ROW_ID)
        .fetch_one(&self.pool)
        .await?;
        let enabled: i64 = row.try_get("enabled")?;
        let click: i64 = row.try_get("click_through_non_interactive_area")?;
        let state = if enabled == 0 {
            OverlayState::Hidden
        } else if click == 1 {
            OverlayState::ClickThrough
        } else {
            OverlayState::Visible
        };
        let session: Option<String> = row.try_get("active_session_id")?;
        let updated: String = row.try_get("updated_at")?;
        Ok(OverlayPreferences {
            state,
            always_on_top: row.try_get::<i64, _>("always_on_top")? == 1,
            opacity: row.try_get("opacity")?,
            width: row.try_get("width")?,
            height: row.try_get("height")?,
            position_x: row.try_get("position_x")?,
            position_y: row.try_get("position_y")?,
            active_session_id: session
                .map(|value| Uuid::parse_str(&value))
                .transpose()
                .map_err(|error| CoreError::Database(sqlx::Error::Decode(Box::new(error))))?,
            show_remote_transcript: row.try_get::<i64, _>("show_remote_transcript")? == 1,
            show_local_transcript: row.try_get::<i64, _>("show_local_transcript")? == 1,
            show_ai_response: row.try_get::<i64, _>("show_ai_response")? == 1,
            undetectable: row.try_get::<i64, _>("undetectable")? == 1,
            updated_at: DateTime::parse_from_rfc3339(&updated)
                .map(|time| time.with_timezone(&Utc))
                .map_err(|error| CoreError::Database(sqlx::Error::Decode(Box::new(error))))?,
        })
    }
}

fn visible_speaker(preferences: &OverlayPreferences, speaker: Speaker) -> bool {
    match speaker {
        Speaker::LocalUser => preferences.show_local_transcript,
        Speaker::Remote | Speaker::System => preferences.show_remote_transcript,
        Speaker::Ai => preferences.show_ai_response,
    }
}

/// Drop an assistant result that belongs to a session the overlay no longer follows.
#[must_use]
pub fn accept_assistant_result(active_session: Uuid, result_session: Uuid) -> bool {
    active_session == result_session
}
