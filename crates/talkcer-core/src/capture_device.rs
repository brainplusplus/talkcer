//! Native Windows capture devices.
//!
//! This module opens the real WASAPI endpoints and keeps them streaming. The
//! microphone is the default capture endpoint. System audio is the default
//! render endpoint opened in loopback mode: WASAPI only enables
//! `AUDCLNT_STREAMFLAGS_LOOPBACK` when a render endpoint is initialized for
//! `Direction::Capture`, which the `wasapi` crate does for us.
//!
//! Each endpoint runs on its own reader thread, so a blocking `wait_for_event`
//! never stalls the async runtime. On every other platform the probe returns a
//! structured [`CoreError::AudioDevice`] instead of panicking.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use crate::capture::{CaptureDevice, CaptureStream, LiveEndpoint};
use crate::error::CoreError;

/// Backend label reported by a successful open.
pub const WASAPI_BACKEND: &str = "wasapi";

/// Sample rate requested from both endpoints.
pub const PROBE_SAMPLE_RATE: u32 = 48_000;

/// Channel count requested from both endpoints.
pub const PROBE_CHANNELS: u16 = 2;

/// How long a one-shot probe reads before reporting.
const PROBE_READ_MS: u64 = 60;

/// Default endpoints opened by a probe.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceProbe {
    /// Backend that produced the devices.
    pub backend: &'static str,
    /// Default microphone and system-loopback endpoints.
    pub devices: Vec<CaptureDevice>,
}

/// Open the default microphone and the default system-audio loopback.
///
/// # Errors
///
/// Returns [`CoreError::AudioDevice`] when the platform has no native capture
/// support or a device cannot be enumerated, initialized, or read.
#[cfg(windows)]
pub fn probe_default_devices() -> Result<DeviceProbe, CoreError> {
    let _ = wasapi::initialize_mta();
    let microphone = probe_endpoint(wasapi::Direction::Capture, "microphone", "capture")?;
    let system_audio = probe_endpoint(wasapi::Direction::Render, "system_audio", "render_loopback")?;
    Ok(DeviceProbe {
        backend: WASAPI_BACKEND,
        devices: vec![microphone, system_audio],
    })
}

/// Native capture is implemented on Windows only.
///
/// # Errors
///
/// Always returns [`CoreError::AudioDevice`] on non-Windows targets.
#[cfg(not(windows))]
pub fn probe_default_devices() -> Result<DeviceProbe, CoreError> {
    Err(CoreError::AudioDevice(
        "native capture is implemented on Windows only".to_owned(),
    ))
}

/// Open both endpoints and start streaming each on its own reader thread.
///
/// # Errors
///
/// Returns [`CoreError::AudioDevice`] when a device cannot be opened, and
/// [`CoreError::InvalidCapture`] when a reader thread cannot be spawned.
#[cfg(windows)]
pub fn start_live_capture() -> Result<Vec<LiveEndpoint>, CoreError> {
    let _ = wasapi::initialize_mta();
    let microphone = spawn_stream(wasapi::Direction::Capture, "microphone", "capture")?;
    let system_audio = spawn_stream(wasapi::Direction::Render, "system_audio", "render_loopback")?;
    Ok(vec![microphone, system_audio])
}

/// Live streaming is implemented on Windows only.
///
/// # Errors
///
/// Always returns [`CoreError::AudioDevice`] on non-Windows targets.
#[cfg(not(windows))]
pub fn start_live_capture() -> Result<Vec<LiveEndpoint>, CoreError> {
    Err(CoreError::AudioDevice(
        "native capture is implemented on Windows only".to_owned(),
    ))
}

/// A reader thread's shared counters.
#[cfg(windows)]
struct StreamShared {
    frames: AtomicU64,
    stop: AtomicBool,
    error: Mutex<Option<String>>,
    /// Raw 16-bit little-endian PCM read since the last drain.
    pcm: Mutex<Vec<u8>>,
}

#[cfg(windows)]
impl StreamShared {
    fn new() -> Self {
        Self {
            frames: AtomicU64::new(0),
            stop: AtomicBool::new(false),
            error: Mutex::new(None),
            pcm: Mutex::new(Vec::new()),
        }
    }
}

#[cfg(windows)]
struct WasapiStream {
    shared: Arc<StreamShared>,
    thread: Mutex<Option<std::thread::JoinHandle<()>>>,
}

#[cfg(windows)]
impl CaptureStream for WasapiStream {
    fn frames(&self) -> u64 {
        self.shared.frames.load(Ordering::Relaxed)
    }

    fn last_error(&self) -> Option<String> {
        self.shared.error.lock().ok().and_then(|guard| guard.clone())
    }

    fn take_pcm(&self) -> Vec<u8> {
        self.shared
            .pcm
            .lock()
            .map(|mut guard| std::mem::take(&mut *guard))
            .unwrap_or_default()
    }

    fn stop(&self) {
        self.shared.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.thread.lock().ok().and_then(|mut guard| guard.take()) {
            let _ = handle.join();
        }
    }
}

#[cfg(windows)]
impl Drop for WasapiStream {
    fn drop(&mut self) {
        self.stop();
    }
}

#[cfg(windows)]
fn spawn_stream(
    endpoint: wasapi::Direction,
    channel: &str,
    direction_label: &str,
) -> Result<LiveEndpoint, CoreError> {
    let shared = Arc::new(StreamShared::new());
    let (tx, rx) = std::sync::mpsc::channel::<Result<CaptureDevice, String>>();
    let thread_shared = Arc::clone(&shared);
    let label = direction_label.to_owned();
    let name = channel.to_owned();

    let handle = std::thread::Builder::new()
        .name(format!("talkcer-{channel}"))
        .spawn(move || {
            let ready = initialize_endpoint(endpoint, &name, &label);
            match ready {
                Ok((device, mut stream)) => {
                    let _ = tx.send(Ok(device));
                    // Stream until asked to stop; read errors are recorded, not fatal.
                    while !thread_shared.stop.load(Ordering::Relaxed) {
                        stream.pump(&thread_shared);
                    }
                    stream.finish();
                }
                Err(message) => {
                    let _ = tx.send(Err(message));
                }
            }
        })
        .map_err(|error| CoreError::InvalidCapture(format!("spawn {channel} reader: {error}")))?;

    match rx.recv() {
        Ok(Ok(device)) => {
            let stream = WasapiStream { shared, thread: Mutex::new(Some(handle)) };
            Ok((device, Box::new(stream)))
        }
        Ok(Err(message)) => {
            let _ = handle.join();
            Err(CoreError::AudioDevice(message))
        }
        Err(_) => {
            let _ = handle.join();
            Err(CoreError::AudioDevice(format!("{channel} reader stopped before opening")))
        }
    }
}

/// An initialized endpoint that can pump packets.
#[cfg(windows)]
struct Endpoint {
    client: wasapi::AudioClient,
    capture: wasapi::AudioCaptureClient,
    handle: wasapi::Handle,
    /// Raw float PCM as WASAPI returns it, drained into `pcm16`.
    queue: std::collections::VecDeque<u8>,
    /// Converted 16-bit little-endian PCM, handed to the pipeline.
    pcm16: Vec<u8>,
    blockalign: usize,
}

#[cfg(windows)]
impl Endpoint {
    fn pump(&mut self, shared: &StreamShared) {
        match self.capture.read_from_device_to_deque(&mut self.queue) {
            Ok(_) => {
                let frames = (self.queue.len() / self.blockalign) as u64;
                if frames > 0 {
                    shared.frames.fetch_add(frames, Ordering::Relaxed);
                    self.pcm16.clear();
                    let samples = self.queue.len() / 4;
                    self.pcm16.reserve(samples * 2);
                    for index in 0..samples {
                        let start = index * 4;
                        let value = f32::from_le_bytes([
                            self.queue[start],
                            self.queue[start + 1],
                            self.queue[start + 2],
                            self.queue[start + 3],
                        ]);
                        let clamped = value.clamp(-1.0, 1.0);
                        let sample = (clamped * f32::from(i16::MAX)).round() as i16;
                        self.pcm16.extend_from_slice(&sample.to_le_bytes());
                    }
                    if let Ok(mut buffer) = shared.pcm.lock() {
                        buffer.extend_from_slice(&self.pcm16);
                    }
                }
            }
            Err(error) => {
                if let Ok(mut slot) = shared.error.lock() {
                    *slot = Some(error.to_string());
                }
            }
        }
        self.queue.clear();
        let _ = self.handle.wait_for_event(20);
    }

    fn finish(&mut self) {
        let _ = self.client.stop_stream();
    }
}

#[cfg(windows)]
fn initialize_endpoint(
    endpoint: wasapi::Direction,
    channel: &str,
    direction_label: &str,
) -> Result<(CaptureDevice, Endpoint), String> {
    use std::collections::VecDeque;

    use wasapi::{get_default_device, SampleType, StreamMode, WaveFormat};

    // Every reader thread needs its own COM apartment.
    let _ = wasapi::initialize_mta();

    let device = get_default_device(&endpoint).map_err(|error| format!("open default device: {error}"))?;
    let name = device.get_friendlyname().map_err(|error| format!("read device name: {error}"))?;
    let mut client = device.get_iaudioclient().map_err(|error| format!("acquire audio client: {error}"))?;

    let format = WaveFormat::new(
        32,
        32,
        &SampleType::Float,
        PROBE_SAMPLE_RATE as usize,
        PROBE_CHANNELS as usize,
        None,
    );
    let blockalign = format.get_blockalign().max(1) as usize;

    let (_, min_period) = client.get_device_period().map_err(|error| format!("read device period: {error}"))?;
    let mode = StreamMode::EventsShared { autoconvert: true, buffer_duration_hns: min_period };
    // A render endpoint initialized for capture is the loopback path; the crate
    // sets AUDCLNT_STREAMFLAGS_LOOPBACK for that combination.
    client
        .initialize_client(&format, &wasapi::Direction::Capture, &mode)
        .map_err(|error| format!("initialize client: {error}"))?;

    let handle = client.set_get_eventhandle().map_err(|error| format!("create event handle: {error}"))?;
    let capture = client.get_audiocaptureclient().map_err(|error| format!("acquire capture client: {error}"))?;
    client.start_stream().map_err(|error| format!("start stream: {error}"))?;

    let device_info = CaptureDevice {
        channel: channel.to_owned(),
        name,
        direction: direction_label.to_owned(),
        state: "active".to_owned(),
        sample_rate: PROBE_SAMPLE_RATE,
        channels: PROBE_CHANNELS,
        frames: 0,
    };
    let queue = VecDeque::with_capacity(blockalign * PROBE_SAMPLE_RATE as usize / 4);
    Ok((
        device_info,
        Endpoint {
            client,
            capture,
            handle,
            queue,
            pcm16: Vec::new(),
            blockalign,
        },
    ))
}

/// One-shot probe: open the endpoint, read briefly, and report.
#[cfg(windows)]
fn probe_endpoint(
    endpoint: wasapi::Direction,
    channel: &str,
    direction_label: &str,
) -> Result<CaptureDevice, CoreError> {
    use std::time::{Duration, Instant};

    let (mut device, mut endpoint) = initialize_endpoint(endpoint, channel, direction_label)
        .map_err(CoreError::AudioDevice)?;
    let shared = StreamShared::new();
    let deadline = Instant::now() + Duration::from_millis(PROBE_READ_MS);
    while Instant::now() < deadline {
        endpoint.pump(&shared);
    }
    device.frames = shared.frames.load(Ordering::Relaxed);
    endpoint.finish();
    Ok(device)
}
