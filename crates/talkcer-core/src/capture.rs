//! Windows live-capture controller.
//!
//! `Start` and `Resume` open the real WASAPI endpoints through
//! [`crate::capture_device`]: the default microphone and the default render
//! endpoint in loopback mode. Buffered audio is segmented and transcribed by
//! [`crate::audio`], keeping the microphone and system-audio transcripts apart.
//! Fixtures can still be injected directly, which is how the HTTP tests exercise
//! the timeline without audio hardware.

use chrono::Utc;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{Row, SqlitePool};
use uuid::Uuid;

use crate::domain::SessionMode;
use crate::error::CoreError;
use crate::session::{SessionRepository, SessionService};
use crate::timeline::{Source, Speaker, TimelineRepository, TimelineService, UpsertTranscript};

/// Persisted capture lifecycle.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureState {
    /// Not capturing.
    Idle,
    /// Both channels are open.
    Capturing,
    /// Channels are open but fixtures are rejected.
    Paused,
    /// A device disconnect was recorded.
    Error,
}

impl CaptureState {
    fn parse(value: &str) -> Result<Self, CoreError> {
        match value {
            "idle" => Ok(Self::Idle),
            "capturing" => Ok(Self::Capturing),
            "paused" => Ok(Self::Paused),
            "error" => Ok(Self::Error),
            other => Err(CoreError::InvalidCapture(other.to_owned())),
        }
    }

    fn as_str(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Capturing => "capturing",
            Self::Paused => "paused",
            Self::Error => "error",
        }
    }
}

/// Capture control request.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureAction {
    /// Open both Windows channels.
    Start,
    /// Stop accepting fixtures without closing history.
    Pause,
    /// Accept fixtures again.
    Resume,
    /// Return to idle.
    Stop,
    /// Record a device disconnect.
    Disconnect,
    /// Leave the error state and return to idle.
    Recover,
}

/// Current capture row.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaptureStatus {
    /// Live session id.
    pub session_id: Uuid,
    /// The only implemented platform.
    pub platform: &'static str,
    /// Lifecycle state.
    pub state: CaptureState,
    /// Disconnect detail, when the state is `error`.
    pub error: Option<String>,
    /// Endpoints opened by the native backend, when capture is running.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub devices: Vec<CaptureDevice>,
}

/// One pipeline tick: the transcripts it stored and any automatic disconnect.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CaptureTick {
    /// Live session id.
    pub session_id: Uuid,
    /// Transcripts stored during this tick, in channel order.
    pub transcripts: Vec<crate::timeline::TranscriptUpsert>,
    /// Set when a reader thread failed and capture moved to `error`.
    pub disconnected: Option<String>,
}

/// One native audio endpoint opened for capture.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CaptureDevice {
    /// `microphone` or `system_audio`.
    pub channel: String,
    /// Endpoint friendly name reported by Windows.
    pub name: String,
    /// `capture` for the microphone or `render_loopback` for system audio.
    pub direction: String,
    /// `active`, or `error` after a read failure.
    pub state: String,
    /// Sample rate the client was initialized with.
    pub sample_rate: u32,
    /// Channel count the client was initialized with.
    pub channels: u16,
    /// Frames read since the endpoint was opened.
    pub frames: u64,
}

/// A running audio endpoint reader.
pub trait CaptureStream: Send {
    /// Frames read since the stream started.
    fn frames(&self) -> u64;
    /// The most recent read error, if any.
    fn last_error(&self) -> Option<String>;
    /// Take the 16-bit little-endian PCM read since the last call.
    fn take_pcm(&self) -> Vec<u8>;
    /// Ask the reader to stop and wait for it to finish.
    fn stop(&self);
}

/// A stream that never produces audio. Used by tests and non-Windows builds.
#[derive(Debug, Default, Clone, Copy)]
pub struct SilentStream;

impl CaptureStream for SilentStream {
    fn frames(&self) -> u64 {
        0
    }

    fn last_error(&self) -> Option<String> {
        None
    }

    fn take_pcm(&self) -> Vec<u8> {
        Vec::new()
    }

    fn stop(&self) {}
}

/// A controllable stream for tests: scripted PCM, frames, and an optional error.
#[derive(Debug, Default)]
pub struct TestStream {
    frames: std::sync::atomic::AtomicU64,
    pcm: std::sync::Mutex<Vec<u8>>,
    error: std::sync::Mutex<Option<String>>,
    stopped: std::sync::atomic::AtomicBool,
}

impl TestStream {
    /// Build a stream over 16-bit little-endian PCM. Frames are `len / 4`
    /// (stereo, 16-bit).
    #[must_use]
    pub fn new(pcm: Vec<u8>) -> Self {
        let frames = (pcm.len() / 4) as u64;
        Self {
            frames: std::sync::atomic::AtomicU64::new(frames),
            pcm: std::sync::Mutex::new(pcm),
            error: std::sync::Mutex::new(None),
            stopped: std::sync::atomic::AtomicBool::new(false),
        }
    }

    /// Record a read error, which is how a device disconnect is surfaced.
    pub fn fail(&self, message: &str) {
        if let Ok(mut slot) = self.error.lock() {
            *slot = Some(message.to_owned());
        }
    }

    /// Append more PCM and advance the frame counter.
    pub fn push(&self, pcm: &[u8]) {
        if let Ok(mut buffer) = self.pcm.lock() {
            buffer.extend_from_slice(pcm);
        }
        self.frames
            .fetch_add((pcm.len() / 4) as u64, std::sync::atomic::Ordering::Relaxed);
    }
}

impl CaptureStream for TestStream {
    fn frames(&self) -> u64 {
        self.frames.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn last_error(&self) -> Option<String> {
        self.error.lock().ok().and_then(|guard| guard.clone())
    }

    fn take_pcm(&self) -> Vec<u8> {
        self.pcm
            .lock()
            .map(|mut guard| std::mem::take(&mut *guard))
            .unwrap_or_default()
    }

    fn stop(&self) {
        self.stopped.store(true, std::sync::atomic::Ordering::Relaxed);
    }
}

impl TestStream {
    /// Whether [`CaptureStream::stop`] was called.
    #[must_use]
    pub fn is_stopped(&self) -> bool {
        self.stopped.load(std::sync::atomic::Ordering::Relaxed)
    }
}

/// A started endpoint: its descriptor plus the reader that owns it.
pub type LiveEndpoint = (CaptureDevice, Box<dyn CaptureStream>);

/// Opens the native audio endpoints used by capture.
pub trait CaptureDeviceProvider: Send + Sync + 'static {
    /// Open the default microphone and the default system-audio loopback
    /// without streaming. Used by `probe_devices`.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::AudioDevice`] when an endpoint cannot be opened.
    fn open(&self) -> Result<Vec<CaptureDevice>, CoreError>;

    /// Open both endpoints and start streaming each of them.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::AudioDevice`] when an endpoint cannot be opened.
    fn start(&self) -> Result<Vec<LiveEndpoint>, CoreError>;
}

/// Provider that opens the real WASAPI endpoints.
#[derive(Debug, Default, Clone, Copy)]
pub struct WasapiDeviceProvider;

impl CaptureDeviceProvider for WasapiDeviceProvider {
    fn open(&self) -> Result<Vec<CaptureDevice>, CoreError> {
        Ok(crate::capture_device::probe_default_devices()?.devices)
    }

    fn start(&self) -> Result<Vec<LiveEndpoint>, CoreError> {
        crate::capture_device::start_live_capture()
    }
}

/// Provider for deterministic tests. It never touches a real device.
#[derive(Debug, Clone)]
pub struct FixtureDeviceProvider {
    devices: Vec<CaptureDevice>,
}

impl FixtureDeviceProvider {
    /// Report the given endpoints.
    #[must_use]
    pub fn new(devices: Vec<CaptureDevice>) -> Self {
        Self { devices }
    }

    /// A standard microphone and system-loopback pair.
    #[must_use]
    pub fn default_pair() -> Self {
        Self {
            devices: vec![
                CaptureDevice {
                    channel: "microphone".to_owned(),
                    name: "Test Microphone".to_owned(),
                    direction: "capture".to_owned(),
                    state: "active".to_owned(),
                    sample_rate: 48_000,
                    channels: 2,
                    frames: 0,
                },
                CaptureDevice {
                    channel: "system_audio".to_owned(),
                    name: "Test Speakers".to_owned(),
                    direction: "render_loopback".to_owned(),
                    state: "active".to_owned(),
                    sample_rate: 48_000,
                    channels: 2,
                    frames: 0,
                },
            ],
        }
    }
}

impl CaptureDeviceProvider for FixtureDeviceProvider {
    fn open(&self) -> Result<Vec<CaptureDevice>, CoreError> {
        Ok(self.devices.clone())
    }

    fn start(&self) -> Result<Vec<LiveEndpoint>, CoreError> {
        Ok(self
            .devices
            .iter()
            .cloned()
            .map(|device| (device, Box::new(SilentStream) as Box<dyn CaptureStream>))
            .collect())
    }
}

/// A test provider that streams scripted PCM and can fail on demand.
///
/// `start` reuses the same [`TestStream`] handles so a test can push more audio
/// or report a disconnect after the streams are already running.
#[derive(Clone, Default)]
pub struct ScriptedDeviceProvider {
    devices: Vec<CaptureDevice>,
    streams: std::sync::Arc<std::sync::Mutex<Vec<std::sync::Arc<TestStream>>>>,
}

impl ScriptedDeviceProvider {
    /// Build a provider over one PCM payload per channel.
    #[must_use]
    pub fn new(pcm_by_channel: Vec<(CaptureDevice, Vec<u8>)>) -> Self {
        let devices: Vec<CaptureDevice> = pcm_by_channel
            .iter()
            .map(|(device, _)| device.clone())
            .collect();
        let streams = pcm_by_channel
            .into_iter()
            .map(|(_, pcm)| std::sync::Arc::new(TestStream::new(pcm)))
            .collect();
        Self {
            devices,
            streams: std::sync::Arc::new(std::sync::Mutex::new(streams)),
        }
    }

    /// A microphone and system-audio pair carrying UTF-8 text.
    #[must_use]
    pub fn spoken_pair(microphone_text: &str, system_text: &str) -> Self {
        let microphone = CaptureDevice {
            channel: "microphone".to_owned(),
            name: "Scripted Microphone".to_owned(),
            direction: "capture".to_owned(),
            state: "active".to_owned(),
            sample_rate: 48_000,
            channels: 2,
            frames: 0,
        };
        let system_audio = CaptureDevice {
            channel: "system_audio".to_owned(),
            name: "Scripted Speakers".to_owned(),
            direction: "render_loopback".to_owned(),
            state: "active".to_owned(),
            sample_rate: 48_000,
            channels: 2,
            frames: 0,
        };
        Self::new(vec![
            (microphone, utf8_pcm(microphone_text)),
            (system_audio, utf8_pcm(system_text)),
        ])
    }

    /// The stream handle for a channel, so a test can push PCM or fail it.
    #[must_use]
    pub fn stream(&self, channel: &str) -> Option<std::sync::Arc<TestStream>> {
        let index = self.devices.iter().position(|device| device.channel == channel)?;
        self.streams.lock().ok()?.get(index).cloned()
    }
}

impl CaptureDeviceProvider for ScriptedDeviceProvider {
    fn open(&self) -> Result<Vec<CaptureDevice>, CoreError> {
        Ok(self.devices.clone())
    }

    fn start(&self) -> Result<Vec<LiveEndpoint>, CoreError> {
        let Ok(streams) = self.streams.lock() else {
            return Err(CoreError::InvalidCapture("scripted streams are unavailable".to_owned()));
        };
        Ok(self
            .devices
            .iter()
            .cloned()
            .zip(streams.iter().cloned())
            .map(|(device, stream)| (device, Box::new(SharedTestStream(stream)) as Box<dyn CaptureStream>))
            .collect())
    }
}

/// A [`TestStream`] handle shared with the test that owns the provider.
#[derive(Debug, Clone)]
struct SharedTestStream(std::sync::Arc<TestStream>);

impl CaptureStream for SharedTestStream {
    fn frames(&self) -> u64 {
        self.0.frames()
    }

    fn last_error(&self) -> Option<String> {
        self.0.last_error()
    }

    fn take_pcm(&self) -> Vec<u8> {
        self.0.take_pcm()
    }

    fn stop(&self) {
        self.0.stop();
    }
}

/// Encode text as stereo 16-bit little-endian PCM.
///
/// Each byte rides in the high byte of the left sample, so the run is loud
/// enough to clear the speech threshold, and the right sample stays silent.
/// [`crate::audio::decode_text`] drops the zero bytes and recovers the text,
/// which is what lets the offline transcriber round-trip a scripted channel.
#[must_use]
pub fn utf8_pcm(text: &str) -> Vec<u8> {
    let mut pcm = Vec::with_capacity(text.len() * 4);
    for byte in text.bytes() {
        pcm.push(0);
        pcm.push(byte);
        pcm.extend_from_slice(&[0, 0]);
    }
    pcm
}

/// Fixture injected into one channel.
#[derive(Debug, Clone, Deserialize)]
pub struct CaptureFixture {
    /// `microphone` for the local user or `system_audio` for the remote person.
    pub channel: String,
    /// Existing utterance for a later partial or final.
    #[serde(default)]
    pub utterance_id: Option<Uuid>,
    /// Fixture transcript.
    pub text: String,
    /// Ordering timestamp.
    pub started_at_ms: i64,
    /// Closing update.
    #[serde(default)]
    pub is_final: bool,
}

/// Live reader threads keyed by session.
type StreamRegistry = std::sync::Arc<std::sync::Mutex<std::collections::HashMap<Uuid, Vec<LiveEndpoint>>>>;

/// Windows-only capture controller.
///
/// Production opens real WASAPI endpoints; tests swap in
/// [`FixtureDeviceProvider`] so they never touch hardware.
#[derive(Clone)]
pub struct CaptureService<S, T> {
    sessions: SessionService<S>,
    timeline: TimelineService<T>,
    pool: SqlitePool,
    provider: std::sync::Arc<dyn CaptureDeviceProvider>,
    resolver: std::sync::Arc<dyn crate::audio::SttResolver>,
    streams: StreamRegistry,
}

impl<S, T> CaptureService<S, T>
where
    S: SessionRepository,
    T: TimelineRepository,
{
    /// Build a controller that opens the real WASAPI devices.
    #[must_use]
    pub fn new(sessions: SessionService<S>, timeline: TimelineService<T>, pool: SqlitePool) -> Self {
        Self::with_provider(sessions, timeline, pool, std::sync::Arc::new(WasapiDeviceProvider))
    }

    /// Build a controller over a custom device provider.
    #[must_use]
    pub fn with_provider(
        sessions: SessionService<S>,
        timeline: TimelineService<T>,
        pool: SqlitePool,
        provider: std::sync::Arc<dyn CaptureDeviceProvider>,
    ) -> Self {
        Self::with_resolver(
            sessions,
            timeline,
            pool,
            provider,
            std::sync::Arc::new(crate::audio::EmbeddedSttResolver),
        )
    }

    /// Build a controller over a custom device provider and STT resolver.
    #[must_use]
    pub fn with_resolver(
        sessions: SessionService<S>,
        timeline: TimelineService<T>,
        pool: SqlitePool,
        provider: std::sync::Arc<dyn CaptureDeviceProvider>,
        resolver: std::sync::Arc<dyn crate::audio::SttResolver>,
    ) -> Self {
        Self {
            sessions,
            timeline,
            pool,
            provider,
            resolver,
            streams: std::sync::Arc::new(std::sync::Mutex::new(std::collections::HashMap::new())),
        }
    }

    /// Apply one lifecycle action.
    ///
    /// `Start` and `Resume` open the native WASAPI devices (default microphone
    /// and default render endpoint in loopback mode) and start a reader thread
    /// per channel. `Pause`, `Stop`, and `Disconnect` stop those readers. The
    /// opened endpoints are returned in [`CaptureStatus::devices`]; a device
    /// failure becomes [`CoreError::AudioDevice`] and leaves the stored state
    /// untouched.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidCapture`] for a non-live session or an illegal
    /// transition, and [`CoreError::AudioDevice`] when a native endpoint cannot
    /// be opened.
    pub async fn control(&self, session_id: Uuid, action: CaptureAction) -> Result<CaptureStatus, CoreError> {
        self.require_live(session_id).await?;
        let current = self.status(session_id).await?;
        let mut devices = Vec::new();
        let (state, error) = match (current.state, action) {
            (CaptureState::Idle | CaptureState::Error, CaptureAction::Start) => {
                devices = self.start_streams(session_id).await?;
                (CaptureState::Capturing, None)
            }
            (CaptureState::Paused, CaptureAction::Resume) => {
                devices = self.start_streams(session_id).await?;
                (CaptureState::Capturing, None)
            }
            (CaptureState::Capturing, CaptureAction::Pause) => {
                self.stop_streams(session_id);
                (CaptureState::Paused, None)
            }
            (_, CaptureAction::Stop) => {
                self.stop_streams(session_id);
                (CaptureState::Idle, None)
            }
            (CaptureState::Capturing | CaptureState::Paused, CaptureAction::Disconnect) => {
                self.stop_streams(session_id);
                (CaptureState::Error, Some("audio device disconnected".to_owned()))
            }
            (CaptureState::Error, CaptureAction::Recover) => (CaptureState::Idle, None),
            (state, _) => {
                return Err(CoreError::InvalidCapture(format!(
                    "cannot apply action while capture is {}",
                    state.as_str()
                )))
            }
        };
        self.store(session_id, state, error.clone()).await?;
        Ok(CaptureStatus { session_id, platform: "windows", state, error, devices })
    }

    /// Advance the pipeline once: transcribe buffered audio and detect a drop.
    ///
    /// This is the tick a caller drives while capture runs. It drains each
    /// channel's buffered PCM, stores the speech runs it can transcribe
    /// (`microphone` as the local user, `system_audio` as the remote person),
    /// and reports an automatic disconnect when a reader thread recorded a
    /// read error. A disconnect stops the readers and moves the session to
    /// [`CaptureState::Error`], which [`Self::control`] can clear with
    /// [`CaptureAction::Recover`].
    ///
    /// # Errors
    ///
    /// Returns a database error when the state row cannot be read or written.
    pub async fn poll(&self, session_id: Uuid) -> Result<CaptureTick, CoreError> {
        if self.status(session_id).await?.state != CaptureState::Capturing {
            return Ok(CaptureTick { session_id, transcripts: Vec::new(), disconnected: None });
        }
        let transcripts = self.ingest(session_id).await;
        let failed = self
            .live_devices(session_id)
            .into_iter()
            .find(|device| device.state == "error");
        let disconnected = match failed {
            Some(device) => {
                self.stop_streams(session_id);
                let message = format!("audio device disconnected: {}", device.channel);
                self.store(session_id, CaptureState::Error, Some(message.clone())).await?;
                Some(message)
            }
            None => None,
        };
        Ok(CaptureTick { session_id, transcripts, disconnected })
    }

    /// Segment every live channel and store the transcripts it can produce.
    async fn ingest(&self, session_id: Uuid) -> Vec<crate::timeline::TranscriptUpsert> {
        // Drain under the lock, then transcribe outside it: the lock must not be
        // held across an await, and streams are not cloneable.
        let drained: Vec<(CaptureDevice, Vec<u8>)> = match self.streams.lock() {
            Ok(guard) => guard
                .get(&session_id)
                .map(|entries| {
                    entries
                        .iter()
                        .map(|(device, stream)| (device.clone(), stream.take_pcm()))
                        .filter(|(_, pcm)| !pcm.is_empty())
                        .collect()
                })
                .unwrap_or_default(),
            Err(_) => Vec::new(),
        };
        // Resolve the session's STT provider once per drain. A session with no
        // STT configuration gets the embedded offline adapter.
        let transcriber = match self.sessions.get(session_id).await {
            Ok(session) => self.resolver.resolve(&session).await.unwrap_or_else(|error| {
                eprintln!("talkcer: stt selection failed for {session_id}: {error}");
                std::sync::Arc::new(crate::audio::EmbeddedTranscriber)
            }),
            Err(error) => {
                eprintln!("talkcer: session {session_id} unavailable for stt: {error}");
                std::sync::Arc::new(crate::audio::EmbeddedTranscriber)
            }
        };
        let mut saved = Vec::new();
        for (device, pcm) in drained {
            let segmenter = crate::audio::Segmenter::for_speech(device.sample_rate, device.channels);
            let started_at_ms = Utc::now().timestamp_millis();
            let (speaker, source) = match device.channel.as_str() {
                "system_audio" => (Speaker::Remote, Source::SystemAudio),
                _ => (Speaker::LocalUser, Source::Microphone),
            };
            for segment in segmenter.segment(started_at_ms, &pcm) {
                match transcriber.transcribe(&segment).await {
                    Ok(Some(text)) => {
                        let result = self
                            .timeline
                            .upsert_transcript(
                                session_id,
                                UpsertTranscript {
                                    utterance_id: None,
                                    speaker,
                                    source,
                                    text,
                                    started_at_ms: segment.started_at_ms,
                                    ended_at_ms: Some(segment.ended_at_ms),
                                    is_final: true,
                                    metadata: Some(json!({
                                        "audio_channel": device.channel,
                                        "platform": "windows",
                                        "rms": segment.rms,
                                        "stt_provider": transcriber.provider(),
                                    })),
                                },
                            )
                            .await;
                        if let Ok(result) = result {
                            saved.push(result);
                        }
                    }
                    Ok(None) => {}
                    Err(error) => {
                        eprintln!(
                            "talkcer: transcription failed for {}: {error}",
                            device.channel
                        );
                    }
                }
            }
        }
        saved
    }

    /// Return the stored state, or idle when capture has not started.
    ///
    /// While capture runs, the returned devices carry live frame counts and a
    /// per-channel error state. Use [`Self::probe_devices`] to open endpoints
    /// without changing the lifecycle.
    ///
    /// # Errors
    ///
    /// Returns a database error when the row cannot be read.
    pub async fn status(&self, session_id: Uuid) -> Result<CaptureStatus, CoreError> {
        let row = sqlx::query("SELECT state, error FROM capture_sessions WHERE session_id = ?")
            .bind(session_id.to_string())
            .fetch_optional(&self.pool)
            .await?;
        let live = self.live_devices(session_id);
        let Some(row) = row else {
            return Ok(CaptureStatus {
                session_id,
                platform: "windows",
                state: CaptureState::Idle,
                error: None,
                devices: live,
            });
        };
        Ok(CaptureStatus {
            session_id,
            platform: "windows",
            state: CaptureState::parse(&row.try_get::<String, _>("state")?)?,
            error: row.try_get("error")?,
            devices: live,
        })
    }

    /// Open the native endpoints without changing the lifecycle state.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::AudioDevice`] when a native endpoint cannot be opened.
    pub fn probe_devices(&self) -> Result<Vec<CaptureDevice>, CoreError> {
        self.provider.open()
    }

    /// Sessions that currently have open reader threads.
    ///
    /// A background pump uses this to decide which sessions to [`Self::poll`].
    #[must_use]
    pub fn live_sessions(&self) -> Vec<Uuid> {
        self.streams
            .lock()
            .map(|guard| guard.keys().copied().collect())
            .unwrap_or_default()
    }

    /// Snapshot the live devices, refreshing frames and error state.
    fn live_devices(&self, session_id: Uuid) -> Vec<CaptureDevice> {
        let Ok(guard) = self.streams.lock() else {
            return Vec::new();
        };
        let Some(entries) = guard.get(&session_id) else {
            return Vec::new();
        };
        entries
            .iter()
            .map(|(device, stream)| {
                let mut snapshot = device.clone();
                snapshot.frames = stream.frames();
                if let Some(error) = stream.last_error() {
                    snapshot.state = "error".to_owned();
                    snapshot.name = format!("{} ({error})", snapshot.name);
                }
                snapshot
            })
            .collect()
    }

    async fn start_streams(&self, session_id: Uuid) -> Result<Vec<CaptureDevice>, CoreError> {
        self.stop_streams(session_id);
        let provider = std::sync::Arc::clone(&self.provider);
        let opened = tokio::task::spawn_blocking(move || provider.start())
            .await
            .map_err(|error| CoreError::InvalidCapture(format!("capture task failed: {error}")))??;
        let devices = opened.iter().map(|(device, _)| device.clone()).collect();
        if let Ok(mut guard) = self.streams.lock() {
            guard.insert(session_id, opened);
        }
        Ok(devices)
    }

    fn stop_streams(&self, session_id: Uuid) {
        let stopped = self
            .streams
            .lock()
            .ok()
            .and_then(|mut guard| guard.remove(&session_id));
        if let Some(entries) = stopped {
            for (_, stream) in entries {
                stream.stop();
            }
        }
    }

    /// Turn one fixture into a separate local or remote transcript.
    ///
    /// # Errors
    ///
    /// Returns [`CoreError::InvalidCapture`] unless capture is currently `capturing`.
    pub async fn inject(
        &self,
        session_id: Uuid,
        fixture: CaptureFixture,
    ) -> Result<crate::timeline::TranscriptUpsert, CoreError> {
        let status = self.status(session_id).await?;
        if status.state != CaptureState::Capturing {
            return Err(CoreError::InvalidCapture(format!(
                "fixture requires capturing, found {}",
                status.state.as_str()
            )));
        }
        let (speaker, source) = match fixture.channel.as_str() {
            "microphone" => (Speaker::LocalUser, Source::Microphone),
            "system_audio" => (Speaker::Remote, Source::SystemAudio),
            other => return Err(CoreError::InvalidCapture(format!("unknown channel {other}"))),
        };
        self.timeline
            .upsert_transcript(
                session_id,
                UpsertTranscript {
                    utterance_id: fixture.utterance_id,
                    speaker,
                    source,
                    text: fixture.text.trim().to_owned(),
                    started_at_ms: fixture.started_at_ms,
                    ended_at_ms: fixture.is_final.then_some(fixture.started_at_ms.saturating_add(1)),
                    is_final: fixture.is_final,
                    metadata: Some(json!({ "audio_channel": fixture.channel, "platform": "windows" })),
                },
            )
            .await
    }

    async fn require_live(&self, session_id: Uuid) -> Result<(), CoreError> {
        let session = self.sessions.get(session_id).await?;
        if session.mode == SessionMode::LiveInterview {
            Ok(())
        } else {
            Err(CoreError::InvalidCapture("capture requires a live_interview session".to_owned()))
        }
    }

    async fn store(&self, session_id: Uuid, state: CaptureState, error: Option<String>) -> Result<(), CoreError> {
        sqlx::query(
            "INSERT INTO capture_sessions (session_id, platform, state, error, updated_at)
             VALUES (?, 'windows', ?, ?, ?)
             ON CONFLICT(session_id) DO UPDATE SET
                state = excluded.state,
                error = excluded.error,
                updated_at = excluded.updated_at",
        )
        .bind(session_id.to_string())
        .bind(state.as_str())
        .bind(error)
        .bind(Utc::now().to_rfc3339())
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}
