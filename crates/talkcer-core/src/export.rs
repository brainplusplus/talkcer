//! Readable session exports.

use serde::Serialize;
use uuid::Uuid;

use crate::domain::Session;
use crate::error::CoreError;
use crate::session::SessionService;
use crate::timeline::{TimelineItem, TimelineQuery, TimelineService};

/// Export encoding.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExportFormat {
    /// Pretty JSON.
    Json,
    /// Plain Markdown.
    Markdown,
}

impl ExportFormat {
    /// Parse `json` or `markdown`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidExport`] for another value.
    pub fn parse(value: &str) -> Result<Self, CoreError> {
        match value {
            "json" => Ok(Self::Json),
            "markdown" => Ok(Self::Markdown),
            other => Err(CoreError::InvalidExport(other.to_owned())),
        }
    }
}

/// One exported document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionExport {
    /// HTTP content type.
    pub content_type: &'static str,
    /// File body.
    pub body: String,
}

#[derive(Serialize)]
struct Document<'a> {
    session: &'a Session,
    items: &'a [TimelineItem],
}

/// Build a JSON or Markdown export for one session.
///
/// # Errors
///
/// Returns [`CoreError::NotFound`] when the session does not exist.
pub async fn export_session<S, T>(
    sessions: &SessionService<S>,
    timeline: &TimelineService<T>,
    session_id: Uuid,
    format: ExportFormat,
) -> Result<SessionExport, CoreError>
where
    S: crate::session::SessionRepository,
    T: crate::timeline::TimelineRepository,
{
    let session = sessions.get(session_id).await?;
    let items = timeline
        .list(session_id, TimelineQuery { limit: 500, before: None })
        .await?
        .items;
    let body = match format {
        ExportFormat::Json => serde_json::to_string_pretty(&Document {
            session: &session,
            items: &items,
        })
        .map_err(|error| CoreError::InvalidExport(error.to_string()))?,
        ExportFormat::Markdown => markdown(&session, &items),
    };
    let content_type = match format {
        ExportFormat::Json => "application/json",
        ExportFormat::Markdown => "text/markdown; charset=utf-8",
    };
    Ok(SessionExport { content_type, body })
}

fn markdown(session: &Session, items: &[TimelineItem]) -> String {
    let mut body = format!(
        "# {}\n\nMode: {}\nStatus: {}\n\n",
        session.title,
        session.mode.as_str(),
        session.status.as_str()
    );
    if items.is_empty() {
        body.push_str("_No timeline items._\n");
        return body;
    }
    for item in items {
        body.push_str(&format!("- {}: {}\n", item.speaker.as_str(), item.text));
    }
    body
}
