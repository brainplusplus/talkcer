//! Talkcer core: domain types, persistence, and session services.

pub mod ai;
pub mod audio;
pub mod audio_selection;
pub mod capture;
pub mod capture_device;
pub mod download;
pub mod domain;
pub mod error;
pub mod export;
pub mod learning;
pub mod limit;
pub mod migrate;
pub mod mock;
pub mod overlay;
pub mod profile;
pub mod remote;
pub mod screen;
pub mod selection;
pub mod session;
pub mod speech;
pub mod thinker;
pub mod timeline;
pub mod voice;

pub use ai::{
    catalog_has, catalog_model, checked_endpoint, discover_models, local_model, local_models, model_catalogs,
    provider_kind,
    run_with_timeout, AiConfiguration, AiConfigurationRepository, AiConfigurationService,
    Capability, LocalModel, ModelCatalog, ModelOption, ProviderCapabilities, ProviderConfig,
    ProviderDescriptor, ProviderKind, ProviderTestResult, SaveAiConfiguration,
    SqliteAiConfigurationRepository,
};
pub use audio_selection::{AudioSelection, AudioSelectionService};
pub use audio::{
    build_transcriber, credential_from_env, decode_text, pcm16_to_wav, rms_i16, select_stt,
    selection_for_model, selection_from_config, ConfiguredSttResolver, EmbeddedSttResolver,
    EmbeddedTranscriber, FixedSttResolver, HttpSttTranscriber, Segment, Segmenter, SttResolver,
    SttSelection, Transcriber,
};
pub use capture::{
    utf8_pcm, AudioDeviceChoice, CaptureAction, CaptureDevice, CaptureDeviceProvider, CaptureFixture, CaptureService,
    CaptureState, CaptureStatus, CaptureStream, CaptureTick, FixtureDeviceProvider, LiveEndpoint,
    ScriptedDeviceProvider, SelectedWasapiProvider, SilentStream, TestStream,
};
pub use capture_device::{
    list_audio_devices, probe_default_devices, start_live_capture, start_selected, DeviceProbe,
};
pub use domain::{Session, SessionMode, SessionStatus};
pub use download::{download_state, finish, DownloadState};
pub use error::CoreError;
pub use export::{export_session, ExportFormat, SessionExport};
pub use learning::{LearningAction, LearningService, LearningTurn, LearningTurnResult};
pub use limit::RateGate;
pub use migrate::{connect_and_migrate, embedded_migrations, MigrationSource};
pub use mock::{MockAction, MockInterviewService, MockTurn, MockTurnResult};
pub use overlay::{
    accept_assistant_result, OverlayPosition, OverlayPreferences, OverlayService, OverlayState,
    OverlayView,
};
pub use profile::{
    cv_prompt, parse_cv_fields, read_cv, CareerEntry, CvDraft, CvSource, EducationEntry, Profile,
    ProfileService, SaveProfile,
};
pub use remote::{PairRequest, Pairing, RemoteAccessService, RemoteStatus};
pub use screen::{
    answer_from_screen, is_pending, mark_sent, pending_screens, record_screen, screen_task,
    HttpVision, ScreenAnswer, ScreenCapturer, ScreenFrame, ScreenIntent, ScreenOutcome,
    VisionReader,
};
pub use selection::{OfferedModel, ProviderSelection, SelectionService};
pub use session::{
    CreateSession, SessionFilter, SessionRepository, SessionService, SessionsByMode,
    SqliteSessionRepository, UpdateSession,
};
pub use speech::{
    build_tts, select_tts, tone_wav, tts_from_config, ConfiguredTtsResolver, EmbeddedTts,
    EmbeddedTtsResolver, FixedTtsResolver, HttpTts, SpeechOutput, Tts, TtsResolver, TtsSelection,
};
pub use thinker::{
    build_thinker, select_model, select_thinker, thinker_from_config, ConfiguredThinkerResolver,
    EmbeddedThinker, EmbeddedThinkerResolver, FixedThinkerResolver, HttpThinker, Thinker,
    ThinkerReply, ThinkerResolver, ThinkerSelection, ThinkerTask,
};
pub use timeline::{
    stub_assistant_text, AssistantAction, AssistantActionRequest, AssistantReply, AssistantResult,
    ItemKind, Purpose, SessionLookup, Source, Speaker, SqliteTimelineRepository, TimelineItem,
    TimelinePage, TimelineQuery, TimelineRepository, TimelineService, TranscriptUpsert,
    UpsertTranscript,
};
pub use voice::{
    mock_stt_event, transcript_text, MockSttEvent, SpeakRequest, SpeechAudio, VoiceAudio,
    VoiceService,
};
