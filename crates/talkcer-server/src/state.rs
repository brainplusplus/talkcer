//! Shared HTTP state.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use std::sync::{Arc, Mutex};

use talkcer_core::{
    AiConfigurationService, AudioSelectionService, CaptureDeviceProvider, CaptureService,
    ConfiguredSttResolver, ConfiguredThinkerResolver, ConfiguredTtsResolver, LearningService,
    MockInterviewService, OverlayService, ProfileService, RateGate, RemoteAccessService,
    SelectedWasapiProvider, SelectionService, SessionService, SqliteAiConfigurationRepository,
    SqliteSessionRepository, SqliteTimelineRepository, TimelineService, VoiceService,
};
use tokio::sync::broadcast;

/// State available to every request handler.
#[derive(Clone)]
pub struct AppState {
    /// Session application service.
    pub sessions: SessionService<SqliteSessionRepository>,
    /// Timeline application service.
    pub timeline: TimelineService<SqliteTimelineRepository>,
    /// Saved STT, thinker, and TTS configuration.
    pub ai: AiConfigurationService<SqliteAiConfigurationRepository>,
    /// Text mock interview controller.
    pub mock: MockInterviewService<SqliteSessionRepository, SqliteTimelineRepository>,
    /// Text learning controller.
    pub learning: LearningService<SqliteSessionRepository, SqliteTimelineRepository>,
    /// Mock browser voice adapter.
    pub voice: VoiceService<SqliteSessionRepository, SqliteTimelineRepository>,
    /// Windows fixture capture controller.
    pub capture: CaptureService<SqliteSessionRepository, SqliteTimelineRepository>,
    /// Pairing gate for a second computer.
    pub remote: RemoteAccessService,
    /// Overlay preferences and the session it follows.
    pub overlay: OverlayService<SqliteTimelineRepository>,
    /// The one personal profile.
    pub profile: ProfileService,
    /// Which models each capability offers in the session picker.
    pub selection: SelectionService,
    /// Remote request limit. Local requests do not consume it.
    pub rate: Arc<tokio::sync::Mutex<RateGate>>,
    /// Broadcast channel for WebSocket subscribers. Payloads are JSON events.
    pub events: broadcast::Sender<String>,
    /// `models/` next to the database file. Created on the first download.
    pub models_dir: PathBuf,
    /// `provider:model` keys whose download is still writing a part file.
    pub downloads: Arc<Mutex<HashSet<String>>>,
    /// The microphone, playback device, and per-channel models capture uses.
    pub audio: AudioSelectionService,
}

impl AppState {
    /// Open SQLite and run pending migrations.
    ///
    /// # Errors
    ///
    /// Returns the underlying core error when the file cannot be opened.
    pub async fn connect(database_path: &Path) -> Result<Self, talkcer_core::CoreError> {
        Self::connect_with_limit(database_path, 30).await
    }

    /// Open SQLite with a custom remote request limit.
    ///
    /// Capture opens the real WASAPI endpoints.
    ///
    /// # Errors
    ///
    /// Returns the underlying core error when the file cannot be opened.
    pub async fn connect_with_limit(
        database_path: &Path,
        remote_limit: u32,
    ) -> Result<Self, talkcer_core::CoreError> {
        let pool = talkcer_core::connect_and_migrate(database_path).await?;
        let audio = AudioSelectionService::new(pool.clone());
        // Load the saved devices once, so capture never starts on the wrong
        // device after a restart.
        audio.load().await?;
        let provider: Arc<dyn CaptureDeviceProvider> =
            Arc::new(SelectedWasapiProvider::from_selection(audio.clone()));
        Self::build(pool, database_path, remote_limit, provider, audio).await
    }

    /// Open SQLite with a custom capture device provider.
    ///
    /// Tests pass a fixture provider so they never touch real audio hardware.
    ///
    /// # Errors
    ///
    /// Returns the underlying core error when the file cannot be opened.
    pub async fn connect_with_capture_provider(
        database_path: &Path,
        remote_limit: u32,
        provider: Arc<dyn CaptureDeviceProvider>,
    ) -> Result<Self, talkcer_core::CoreError> {
        let pool = talkcer_core::connect_and_migrate(database_path).await?;
        let audio = AudioSelectionService::new(pool.clone());
        audio.load().await?;
        Self::build(pool, database_path, remote_limit, provider, audio).await
    }

    /// Assemble the state from an open pool and a capture provider.
    async fn build(
        pool: sqlx::SqlitePool,
        database_path: &Path,
        remote_limit: u32,
        provider: Arc<dyn CaptureDeviceProvider>,
        audio: AudioSelectionService,
    ) -> Result<Self, talkcer_core::CoreError> {
        let models_dir = database_path
            .parent()
            .unwrap_or(Path::new("."))
            .join("models");
        let (events, _) = broadcast::channel(256);
        let sessions = SessionService::new(SqliteSessionRepository::new(pool.clone()));
        // The timeline resolves each session's thinker from the saved AI
        // configurations, falling back to the embedded offline adapter. Mock
        // and learning reuse the same timeline, so they inherit the choice.
        let thinker: Arc<dyn talkcer_core::ThinkerResolver> =
            Arc::new(ConfiguredThinkerResolver::new(Arc::new(
                SqliteAiConfigurationRepository::new(pool.clone()),
            )));
        let timeline =
            TimelineService::with_thinker(SqliteTimelineRepository::new(pool.clone()), thinker)
                .with_sessions(sessions.clone());
        // Capture resolves each session's STT provider from the saved AI
        // configurations, and each channel's model from the Audio page.
        let resolver: Arc<dyn talkcer_core::SttResolver> = Arc::new(
            ConfiguredSttResolver::new(Arc::new(SqliteAiConfigurationRepository::new(
                pool.clone(),
            )))
            .with_audio_selection(Arc::new(audio.clone())),
        );
        // Voice resolves each session's TTS provider the same way.
        let tts: Arc<dyn talkcer_core::TtsResolver> = Arc::new(ConfiguredTtsResolver::new(
            Arc::new(SqliteAiConfigurationRepository::new(pool.clone())),
        ));
        Ok(Self {
            mock: MockInterviewService::new(sessions.clone(), timeline.clone()),
            learning: LearningService::new(sessions.clone(), timeline.clone()),
            voice: VoiceService::with_tts(sessions.clone(), timeline.clone(), tts),
            capture: CaptureService::with_resolver(
                sessions.clone(),
                timeline.clone(),
                pool.clone(),
                provider,
                resolver,
            ),
            remote: RemoteAccessService::new(pool.clone()),
            overlay: OverlayService::new(pool.clone(), timeline.clone()),
            profile: ProfileService::new(pool.clone()),
            selection: SelectionService::new(pool.clone()),
            rate: Arc::new(tokio::sync::Mutex::new(RateGate::new(remote_limit))),
            models_dir,
            downloads: Arc::new(Mutex::new(HashSet::new())),
            sessions,
            timeline,
            ai: AiConfigurationService::new(SqliteAiConfigurationRepository::new(pool)),
            events,
            audio,
        })
    }

    /// Publish one event. A lack of subscribers is not an error.
    pub fn publish(&self, event: &serde_json::Value) {
        let _ = self.events.send(event.to_string());
    }
}
