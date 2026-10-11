//! Talkcer core: domain types, persistence, and session services.

pub mod ai;
pub mod capture;
pub mod domain;
pub mod error;
pub mod export;
pub mod learning;
pub mod limit;
pub mod migrate;
pub mod mock;
pub mod overlay;
pub mod voice;
pub mod remote;
pub mod session;
pub mod timeline;

pub use ai::{
    run_with_timeout,
    AiConfiguration, AiConfigurationRepository, AiConfigurationService, Capability, ProviderCapabilities,
    ProviderConfig, ProviderDescriptor, ProviderKind, ProviderTestResult, SaveAiConfiguration,
    SqliteAiConfigurationRepository,
};
pub use capture::{CaptureAction, CaptureFixture, CaptureService, CaptureState, CaptureStatus};
pub use domain::{Session, SessionMode, SessionStatus};
pub use remote::{PairRequest, Pairing, RemoteAccessService, RemoteStatus};
pub use mock::{MockAction, MockInterviewService, MockTurn, MockTurnResult};
pub use error::CoreError;
pub use export::{export_session, ExportFormat, SessionExport};
pub use limit::RateGate;
pub use overlay::{
    accept_assistant_result, OverlayPosition, OverlayPreferences, OverlayService, OverlayState, OverlayView,
};
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
