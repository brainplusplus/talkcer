//! Talkcer core: domain types, persistence, and session services.

pub mod ai;
pub mod audio;
pub mod capture;
pub mod capture_device;
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
pub mod speech;
pub mod thinker;
pub mod timeline;

pub use ai::{
    run_with_timeout,
    AiConfiguration, AiConfigurationRepository, AiConfigurationService, Capability, ProviderCapabilities,
    ProviderConfig, ProviderDescriptor, ProviderKind, ProviderTestResult, SaveAiConfiguration,
    SqliteAiConfigurationRepository,
};
pub use capture::{
    utf8_pcm, CaptureAction, CaptureDevice, CaptureDeviceProvider, CaptureFixture, CaptureService,
    CaptureState, CaptureStatus, CaptureStream, CaptureTick, FixtureDeviceProvider, LiveEndpoint,
    ScriptedDeviceProvider, SilentStream, TestStream, WasapiDeviceProvider,
};
pub use capture_device::{probe_default_devices, start_live_capture, DeviceProbe};
pub use audio::{
    build_transcriber, credential_from_env, decode_text, pcm16_to_wav, rms_i16, select_stt,
    selection_from_config, ConfiguredSttResolver, EmbeddedSttResolver, EmbeddedTranscriber,
    FixedSttResolver, HttpSttTranscriber, Segment, Segmenter, SttResolver, SttSelection,
    Transcriber,
};
pub use thinker::{
    build_thinker, select_thinker, thinker_from_config, ConfiguredThinkerResolver,
    EmbeddedThinker, EmbeddedThinkerResolver, FixedThinkerResolver, HttpThinker, Thinker,
    ThinkerReply, ThinkerResolver, ThinkerSelection, ThinkerTask,
};
pub use speech::{
    build_tts, select_tts, tone_wav, tts_from_config, ConfiguredTtsResolver, EmbeddedTts,
    EmbeddedTtsResolver, FixedTtsResolver, HttpTts, SpeechOutput, Tts, TtsResolver, TtsSelection,
};
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
    stub_assistant_text, AssistantAction, AssistantActionRequest, AssistantReply, AssistantResult,
    ItemKind, Purpose, SessionLookup, Source, Speaker, SqliteTimelineRepository, TimelineItem,
    TimelinePage, TimelineQuery, TimelineRepository, TimelineService, TranscriptUpsert,
    UpsertTranscript,
};
