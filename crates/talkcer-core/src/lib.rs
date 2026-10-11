//! Talkcer core: domain types, persistence, and session services.

pub mod ai;
pub mod domain;
pub mod error;
pub mod learning;
pub mod migrate;
pub mod mock;
pub mod voice;
pub mod session;
pub mod timeline;

pub use ai::{
    AiConfiguration, AiConfigurationRepository, AiConfigurationService, Capability, ProviderCapabilities,
    ProviderConfig, ProviderDescriptor, ProviderKind, ProviderTestResult, SaveAiConfiguration,
    SqliteAiConfigurationRepository,
};
pub use domain::{Session, SessionMode, SessionStatus};
pub use mock::{MockAction, MockInterviewService, MockTurn, MockTurnResult};
pub use error::CoreError;
pub use learning::{LearningAction, LearningService, LearningTurn, LearningTurnResult};
pub use voice::{mock_stt_event, transcript_text, MockSttEvent, SpeakRequest, SpeechAudio, VoiceAudio, VoiceService};
pub use migrate::{connect_and_migrate, embedded_migrations, MigrationSource};
pub use session::{
    CreateSession, SessionFilter, SessionRepository, SessionService, SessionsByMode,
    SqliteSessionRepository, UpdateSession,
};
pub use timeline::{
    stub_assistant_text, AssistantAction, AssistantActionRequest, AssistantResult, ItemKind,
    Purpose, Source, Speaker, SqliteTimelineRepository, TimelineItem, TimelinePage, TimelineQuery,
    TimelineRepository, TimelineService, TranscriptUpsert, UpsertTranscript,
};
