//! Timeline persistence.
//!
//! A transcript utterance is one `timeline_items` row. Partial text updates
//! that row. Final text replaces it and closes the utterance. Chat messages
//! and the Task 3 assistant stub are separate rows.

use std::sync::Arc;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::error::CoreError;

const COLUMNS: &str = "id, session_id, kind, speaker, source, purpose, text, started_at_ms, ended_at_ms, is_final, metadata, created_at";

/// Kind of one timeline row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemKind {
    /// Speech transcript, partial or final.
    Transcript,
    /// Text entered by the user.
    UserMessage,
    /// Assistant output.
    AiMessage,
    /// Evaluation left for a later task.
    Feedback,
    /// Non-conversational marker.
    SystemEvent,
}

impl ItemKind {
    /// Parse a stored kind.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidItemKind`] for an unknown value.
    pub fn parse(value: &str) -> Result<Self, CoreError> {
        match value {
            "transcript" => Ok(Self::Transcript),
            "user_message" => Ok(Self::UserMessage),
            "ai_message" => Ok(Self::AiMessage),
            "feedback" => Ok(Self::Feedback),
            "system_event" => Ok(Self::SystemEvent),
            other => Err(CoreError::InvalidItemKind(other.to_owned())),
        }
    }

    /// Wire value stored in SQLite.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Transcript => "transcript",
            Self::UserMessage => "user_message",
            Self::AiMessage => "ai_message",
            Self::Feedback => "feedback",
            Self::SystemEvent => "system_event",
        }
    }
}

/// Who produced a timeline row.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Speaker {
    /// The person using Talkcer.
    LocalUser,
    /// The other person in a live conversation.
    Remote,
    /// The assistant.
    Ai,
    /// Talkcer itself.
    System,
}

impl Speaker {
    /// Parse a wire speaker.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidSpeaker`] for an unknown value.
    pub fn parse(value: &str) -> Result<Self, CoreError> {
        match value {
            "local_user" => Ok(Self::LocalUser),
            "remote" => Ok(Self::Remote),
            "ai" => Ok(Self::Ai),
            "system" => Ok(Self::System),
            other => Err(CoreError::InvalidSpeaker(other.to_owned())),
        }
    }

    /// Wire value stored in SQLite.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::LocalUser => "local_user",
            Self::Remote => "remote",
            Self::Ai => "ai",
            Self::System => "system",
        }
    }
}

/// Where a timeline row came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Microphone audio.
    Microphone,
    /// System or remote meeting audio.
    SystemAudio,
    /// Typed text.
    Text,
    /// Text produced by the assistant.
    AiGenerated,
}

impl Source {
    /// Parse a wire source.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidSource`] for an unknown value.
    pub fn parse(value: &str) -> Result<Self, CoreError> {
        match value {
            "microphone" => Ok(Self::Microphone),
            "system_audio" => Ok(Self::SystemAudio),
            "text" => Ok(Self::Text),
            "ai_generated" => Ok(Self::AiGenerated),
            other => Err(CoreError::InvalidSource(other.to_owned())),
        }
    }

    /// Wire value stored in SQLite.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Microphone => "microphone",
            Self::SystemAudio => "system_audio",
            Self::Text => "text",
            Self::AiGenerated => "ai_generated",
        }
    }
}

/// Why a timeline row exists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Purpose {
    /// Ordinary conversation content.
    Conversation,
    /// A request sent to the assistant.
    AssistantInstruction,
    /// The assistant's answer.
    AssistantResponse,
}

impl Purpose {
    /// Parse a stored purpose.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidPurpose`] for an unknown value.
    pub fn parse(value: &str) -> Result<Self, CoreError> {
        match value {
            "conversation" => Ok(Self::Conversation),
            "assistant_instruction" => Ok(Self::AssistantInstruction),
            "assistant_response" => Ok(Self::AssistantResponse),
            other => Err(CoreError::InvalidPurpose(other.to_owned())),
        }
    }

    /// Wire value stored in SQLite.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Conversation => "conversation",
            Self::AssistantInstruction => "assistant_instruction",
            Self::AssistantResponse => "assistant_response",
        }
    }
}

/// Assistant action supported by the API contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssistantAction {
    /// Explain the targeted utterance.
    Explain,
    /// Translate the targeted utterance.
    Translate,
    /// Summarize the targeted utterance.
    Summarize,
    /// Suggest talking points.
    SuggestPoints,
    /// Run a caller-supplied instruction.
    CustomInstruction,
}

impl AssistantAction {
    /// Parse a wire action.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidAction`] for an unknown value.
    pub fn parse(value: &str) -> Result<Self, CoreError> {
        match value {
            "explain" => Ok(Self::Explain),
            "translate" => Ok(Self::Translate),
            "summarize" => Ok(Self::Summarize),
            "suggest_points" => Ok(Self::SuggestPoints),
            "custom_instruction" => Ok(Self::CustomInstruction),
            other => Err(CoreError::InvalidAction(other.to_owned())),
        }
    }

    /// Wire value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Explain => "explain",
            Self::Translate => "translate",
            Self::Summarize => "summarize",
            Self::SuggestPoints => "suggest_points",
            Self::CustomInstruction => "custom_instruction",
        }
    }
}

/// Fixed Task 3 assistant text. No provider is called.
#[must_use]
pub fn stub_assistant_text(action: AssistantAction, utterance_id: Uuid) -> String {
    format!(
        "Stub response for action \"{}\" targeting utterance {utterance_id}.",
        action.as_str()
    )
}

/// One persisted timeline row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimelineItem {
    /// Row id. For a transcript this is also the utterance id.
    pub id: Uuid,
    /// Owning session.
    pub session_id: Uuid,
    /// Row kind.
    pub kind: ItemKind,
    /// Speaker.
    pub speaker: Speaker,
    /// Source.
    pub source: Source,
    /// Purpose.
    pub purpose: Purpose,
    /// Current text. Final transcript text replaces the partial text.
    pub text: String,
    /// Ordering timestamp.
    pub started_at_ms: i64,
    /// Exclusive end, when known.
    pub ended_at_ms: Option<i64>,
    /// `true` once a transcript is final or a chat row is complete.
    pub is_final: bool,
    /// Extra JSON. Assistant rows store the target utterance here.
    pub metadata: Value,
    /// Insert time.
    pub created_at: DateTime<Utc>,
    /// Present only for transcript rows. Equal to [`Self::id`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub utterance_id: Option<Uuid>,
}

/// Create or update one transcript utterance.
#[derive(Debug, Clone, Deserialize)]
pub struct UpsertTranscript {
    /// Existing utterance. Omitted values create a new id.
    pub utterance_id: Option<Uuid>,
    /// Speaker.
    pub speaker: Speaker,
    /// Audio or text source.
    pub source: Source,
    /// Partial or final text.
    pub text: String,
    /// Ordering timestamp supplied by the caller.
    pub started_at_ms: i64,
    /// Optional end timestamp.
    #[serde(default)]
    pub ended_at_ms: Option<i64>,
    /// `false` for a partial, `true` for the closing update.
    #[serde(default)]
    pub is_final: bool,
}

/// Result of creating or updating an utterance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TranscriptUpsert {
    /// Stored row.
    pub item: TimelineItem,
    /// `true` when this call inserted the row.
    pub created: bool,
}

/// Page query for timeline history.
#[derive(Debug, Clone)]
pub struct TimelineQuery {
    /// Maximum rows. Must be `1..=500`.
    pub limit: u32,
    /// Return rows strictly older than this item.
    pub before: Option<Uuid>,
}

/// One page of timeline history, oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TimelinePage {
    /// Rows ordered by `started_at_ms`, then `created_at`, then id.
    pub items: Vec<TimelineItem>,
}

/// Request for the local assistant stub.
#[derive(Debug, Clone)]
pub struct AssistantActionRequest {
    /// Requested action.
    pub action: AssistantAction,
    /// Transcript utterance to attach in metadata.
    pub target_id: Uuid,
    /// Optional output language. Stored, not interpreted.
    pub language: Option<String>,
}

/// Rows written by one stub assistant action.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AssistantResult {
    /// Audit row for the request.
    pub instruction: TimelineItem,
    /// Separate assistant response row.
    pub message: TimelineItem,
}

/// Persistence port for timeline rows.
#[async_trait::async_trait]
pub trait TimelineRepository: Send + Sync {
    /// Insert a transcript or update it while it is still partial.
    async fn upsert_transcript(
        &self,
        session_id: Uuid,
        input: UpsertTranscript,
    ) -> Result<TranscriptUpsert, CoreError>;

    /// List one page of a session.
    async fn list(&self, session_id: Uuid, query: &TimelineQuery) -> Result<TimelinePage, CoreError>;

    /// Store a user message.
    async fn append_message(
        &self,
        session_id: Uuid,
        text: String,
        metadata: Value,
    ) -> Result<TimelineItem, CoreError>;

    /// Store the instruction and the stub assistant response.
    async fn record_assistant(
        &self,
        session_id: Uuid,
        request: AssistantActionRequest,
    ) -> Result<AssistantResult, CoreError>;

    /// Store one completed timeline row of the requested kind.
    async fn append_item(
        &self,
        session_id: Uuid,
        kind: ItemKind,
        speaker: Speaker,
        purpose: Purpose,
        text: String,
        metadata: Value,
    ) -> Result<TimelineItem, CoreError>;
}

/// SQLite timeline store.
#[derive(Clone)]
pub struct SqliteTimelineRepository {
    pool: SqlitePool,
}

impl SqliteTimelineRepository {
    /// Wrap a migrated pool.
    #[must_use]
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

#[async_trait::async_trait]
impl TimelineRepository for SqliteTimelineRepository {
    async fn upsert_transcript(
        &self,
        session_id: Uuid,
        input: UpsertTranscript,
    ) -> Result<TranscriptUpsert, CoreError> {
        if input.started_at_ms < 0 {
            return Err(CoreError::InvalidTimestamp);
        }
        let mut tx = self.pool.begin().await?;
        ensure_session(&mut tx, session_id).await?;
        let id = input.utterance_id.unwrap_or_else(Uuid::now_v7);
        let existing = find_item(&mut tx, id).await?;
        let created = if let Some(existing) = existing {
            if existing.session_id != session_id {
                return Err(CoreError::UtteranceSessionMismatch);
            }
            if existing.is_final {
                return Err(CoreError::UtteranceClosed);
            }
            sqlx::query(
                "UPDATE timeline_items
                 SET text = ?, is_final = ?, ended_at_ms = COALESCE(?, ended_at_ms)
                 WHERE id = ? AND session_id = ? AND is_final = 0",
            )
            .bind(&input.text)
            .bind(i64::from(input.is_final))
            .bind(input.ended_at_ms)
            .bind(id.to_string())
            .bind(session_id.to_string())
            .execute(&mut *tx)
            .await?;
            false
        } else {
            let now = Utc::now();
            insert_item(
                &mut tx,
                &new_item(
                    id,
                    session_id,
                    ItemKind::Transcript,
                    input.speaker,
                    input.source,
                    Purpose::Conversation,
                    input.text,
                    input.started_at_ms,
                    input.ended_at_ms,
                    input.is_final,
                    json!({}),
                    now,
                ),
            )
            .await?;
            true
        };
        let item = find_item(&mut tx, id).await?.ok_or(CoreError::ItemNotFound)?;
        tx.commit().await?;
        Ok(TranscriptUpsert { item, created })
    }

    async fn list(&self, session_id: Uuid, query: &TimelineQuery) -> Result<TimelinePage, CoreError> {
        if !(1..=500).contains(&query.limit) {
            return Err(CoreError::InvalidLimit);
        }
        let mut tx = self.pool.begin().await?;
        ensure_session(&mut tx, session_id).await?;
        let rows = if let Some(before) = query.before {
            let cursor = find_item(&mut tx, before).await?.ok_or(CoreError::ItemNotFound)?;
            if cursor.session_id != session_id {
                return Err(CoreError::ItemNotFound);
            }
            sqlx::query(&format!(
                "SELECT {COLUMNS} FROM timeline_items
                 WHERE session_id = ?
                   AND (
                     started_at_ms < ?
                     OR (started_at_ms = ? AND created_at < ?)
                     OR (started_at_ms = ? AND created_at = ? AND id < ?)
                   )
                 ORDER BY started_at_ms DESC, created_at DESC, id DESC
                 LIMIT ?"
            ))
            .bind(session_id.to_string())
            .bind(cursor.started_at_ms)
            .bind(cursor.started_at_ms)
            .bind(cursor.created_at.to_rfc3339())
            .bind(cursor.started_at_ms)
            .bind(cursor.created_at.to_rfc3339())
            .bind(cursor.id.to_string())
            .bind(i64::from(query.limit))
            .fetch_all(&mut *tx)
            .await?
        } else {
            sqlx::query(&format!(
                "SELECT {COLUMNS} FROM timeline_items
                 WHERE session_id = ?
                 ORDER BY started_at_ms DESC, created_at DESC, id DESC
                 LIMIT ?"
            ))
            .bind(session_id.to_string())
            .bind(i64::from(query.limit))
            .fetch_all(&mut *tx)
            .await?
        };
        let mut items = rows.into_iter().map(map_item).collect::<Result<Vec<_>, _>>()?;
        items.reverse();
        Ok(TimelinePage { items })
    }

    async fn append_message(
        &self,
        session_id: Uuid,
        text: String,
        metadata: Value,
    ) -> Result<TimelineItem, CoreError> {
        let text = text.trim().to_owned();
        if text.is_empty() {
            return Err(CoreError::EmptyText);
        }
        let mut tx = self.pool.begin().await?;
        ensure_session(&mut tx, session_id).await?;
        let now = Utc::now();
        let item = new_item(
            Uuid::now_v7(),
            session_id,
            ItemKind::UserMessage,
            Speaker::LocalUser,
            Source::Text,
            Purpose::Conversation,
            text,
            now.timestamp_millis(),
            None,
            true,
            if metadata.is_null() { json!({}) } else { metadata },
            now,
        );
        insert_item(&mut tx, &item).await?;
        tx.commit().await?;
        Ok(item)
    }

    async fn record_assistant(
        &self,
        session_id: Uuid,
        request: AssistantActionRequest,
    ) -> Result<AssistantResult, CoreError> {
        let mut tx = self.pool.begin().await?;
        ensure_session(&mut tx, session_id).await?;
        let target = find_item(&mut tx, request.target_id)
            .await?
            .ok_or(CoreError::ItemNotFound)?;
        if target.session_id != session_id || target.kind != ItemKind::Transcript {
            return Err(CoreError::InvalidTarget);
        }
        let now = Utc::now();
        let mut metadata = json!({
            "action": request.action.as_str(),
            "target": { "kind": "utterance", "id": request.target_id }
        });
        if let Some(language) = request.language {
            metadata["language"] = json!(language);
        }
        let instruction = new_item(
            Uuid::now_v7(),
            session_id,
            ItemKind::UserMessage,
            Speaker::LocalUser,
            Source::Text,
            Purpose::AssistantInstruction,
            request.action.as_str().to_owned(),
            now.timestamp_millis(),
            None,
            true,
            metadata.clone(),
            now,
        );
        let mut ai_metadata = metadata;
        ai_metadata["stub"] = json!(true);
        let message = new_item(
            Uuid::now_v7(),
            session_id,
            ItemKind::AiMessage,
            Speaker::Ai,
            Source::AiGenerated,
            Purpose::AssistantResponse,
            stub_assistant_text(request.action, request.target_id),
            (now + Duration::milliseconds(1)).timestamp_millis(),
            None,
            true,
            ai_metadata,
            now + Duration::milliseconds(1),
        );
        insert_item(&mut tx, &instruction).await?;
        insert_item(&mut tx, &message).await?;
        tx.commit().await?;
        Ok(AssistantResult {
            instruction,
            message,
        })
    }

    async fn append_item(
        &self,
        session_id: Uuid,
        kind: ItemKind,
        speaker: Speaker,
        purpose: Purpose,
        text: String,
        metadata: Value,
    ) -> Result<TimelineItem, CoreError> {
        let text = text.trim().to_owned();
        if text.is_empty() {
            return Err(CoreError::EmptyText);
        }
        let source = match speaker {
            Speaker::Ai => Source::AiGenerated,
            Speaker::System => Source::Text,
            Speaker::LocalUser | Speaker::Remote => Source::Text,
        };
        let mut tx = self.pool.begin().await?;
        ensure_session(&mut tx, session_id).await?;
        let now = Utc::now();
        let item = new_item(
            Uuid::now_v7(),
            session_id,
            kind,
            speaker,
            source,
            purpose,
            text,
            now.timestamp_millis(),
            Some(now.timestamp_millis()),
            true,
            if metadata.is_object() { metadata } else { json!({}) },
            now,
        );
        insert_item(&mut tx, &item).await?;
        tx.commit().await?;
        Ok(item)
    }
}

/// Application service for timeline reads and writes.
#[derive(Clone)]
pub struct TimelineService<R> {
    repository: Arc<R>,
}

impl<R> TimelineService<R>
where
    R: TimelineRepository,
{
    /// Wrap a repository.
    #[must_use]
    pub fn new(repository: R) -> Self {
        Self {
            repository: Arc::new(repository),
        }
    }

    /// Create or update a transcript utterance.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::NotFound`] when the session does not exist,
    /// [`CoreError::UtteranceClosed`] when the utterance is already final,
    /// and [`CoreError::UtteranceSessionMismatch`] when the id belongs elsewhere.
    pub async fn upsert_transcript(
        &self,
        session_id: Uuid,
        input: UpsertTranscript,
    ) -> Result<TranscriptUpsert, CoreError> {
        self.repository.upsert_transcript(session_id, input).await
    }

    /// List timeline history, oldest first within the page.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidLimit`] when `limit` is outside `1..=500`,
    /// [`CoreError::NotFound`] when the session does not exist, and
    /// [`CoreError::ItemNotFound`] when `before` is not in the session.
    pub async fn list(
        &self,
        session_id: Uuid,
        query: TimelineQuery,
    ) -> Result<TimelinePage, CoreError> {
        self.repository.list(session_id, &query).await
    }

    /// Append a user chat message.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::EmptyText`] when the text is blank and
    /// [`CoreError::NotFound`] when the session does not exist.
    pub async fn append_message(
        &self,
        session_id: Uuid,
        text: String,
        metadata: Value,
    ) -> Result<TimelineItem, CoreError> {
        self.repository
            .append_message(session_id, text, metadata)
            .await
    }

    /// Record a local stub assistant response without calling a provider.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::ItemNotFound`] when the utterance does not exist and
    /// [`CoreError::InvalidTarget`] when it is not a transcript in this session.
    pub async fn assistant_action(
        &self,
        session_id: Uuid,
        request: AssistantActionRequest,
    ) -> Result<AssistantResult, CoreError> {
        self.repository.record_assistant(session_id, request).await
    }

    /// Append one completed timeline row.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::EmptyText`] when the text is blank and
    /// [`CoreError::NotFound`] when the session does not exist.
    pub async fn append_item(
        &self,
        session_id: Uuid,
        kind: ItemKind,
        speaker: Speaker,
        purpose: Purpose,
        text: String,
        metadata: Value,
    ) -> Result<TimelineItem, CoreError> {
        self.repository
            .append_item(session_id, kind, speaker, purpose, text, metadata)
            .await
    }
}

fn new_item(
    id: Uuid,
    session_id: Uuid,
    kind: ItemKind,
    speaker: Speaker,
    source: Source,
    purpose: Purpose,
    text: String,
    started_at_ms: i64,
    ended_at_ms: Option<i64>,
    is_final: bool,
    metadata: Value,
    created_at: DateTime<Utc>,
) -> TimelineItem {
    TimelineItem {
        utterance_id: (kind == ItemKind::Transcript).then_some(id),
        id,
        session_id,
        kind,
        speaker,
        source,
        purpose,
        text,
        started_at_ms,
        ended_at_ms,
        is_final,
        metadata,
        created_at,
    }
}

async fn ensure_session(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    session_id: Uuid,
) -> Result<(), CoreError> {
    let found: Option<(String,)> = sqlx::query_as("SELECT id FROM sessions WHERE id = ?")
        .bind(session_id.to_string())
        .fetch_optional(&mut **tx)
        .await?;
    if found.is_some() {
        Ok(())
    } else {
        Err(CoreError::NotFound)
    }
}

async fn find_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: Uuid,
) -> Result<Option<TimelineItem>, CoreError> {
    let row = sqlx::query(&format!("SELECT {COLUMNS} FROM timeline_items WHERE id = ?"))
        .bind(id.to_string())
        .fetch_optional(&mut **tx)
        .await?;
    row.map(map_item).transpose()
}

async fn insert_item(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    item: &TimelineItem,
) -> Result<(), CoreError> {
    sqlx::query(
        "INSERT INTO timeline_items
         (id, session_id, kind, speaker, source, purpose, text, started_at_ms, ended_at_ms, is_final, metadata, created_at)
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(item.id.to_string())
    .bind(item.session_id.to_string())
    .bind(item.kind.as_str())
    .bind(item.speaker.as_str())
    .bind(item.source.as_str())
    .bind(item.purpose.as_str())
    .bind(&item.text)
    .bind(item.started_at_ms)
    .bind(item.ended_at_ms)
    .bind(i64::from(item.is_final))
    .bind(item.metadata.to_string())
    .bind(item.created_at.to_rfc3339())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn map_item(row: sqlx::sqlite::SqliteRow) -> Result<TimelineItem, CoreError> {
    let id = parse_uuid(row.try_get("id")?)?;
    let session_id = parse_uuid(row.try_get("session_id")?)?;
    let kind = ItemKind::parse(&row.try_get::<String, _>("kind")?)?;
    let metadata: String = row.try_get("metadata")?;
    let created_at: String = row.try_get("created_at")?;
    let is_final: i64 = row.try_get("is_final")?;
    Ok(new_item(
        id,
        session_id,
        kind,
        Speaker::parse(&row.try_get::<String, _>("speaker")?)?,
        Source::parse(&row.try_get::<String, _>("source")?)?,
        Purpose::parse(&row.try_get::<String, _>("purpose")?)?,
        row.try_get("text")?,
        row.try_get("started_at_ms")?,
        row.try_get("ended_at_ms")?,
        is_final != 0,
        serde_json::from_str(&metadata).unwrap_or_else(|_| json!({})),
        DateTime::parse_from_rfc3339(&created_at)
            .map(|time| time.with_timezone(&Utc))
            .map_err(|error| CoreError::Database(sqlx::Error::Decode(Box::new(error))))?,
        ))
}

fn parse_uuid(value: String) -> Result<Uuid, CoreError> {
    Uuid::parse_str(&value).map_err(|error| CoreError::Database(sqlx::Error::Decode(Box::new(error))))
}
