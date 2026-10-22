//! Native Windows device tests.
//!
//! The probe opens real WASAPI endpoints, so the assertion tolerates a machine
//! without audio hardware by accepting a structured [`CoreError::AudioDevice`]
//! instead of a panic. When devices exist, both channels must be distinct and
//! initialized with the requested format.

use std::time::{Duration, Instant};

use talkcer_core::{probe_default_devices, start_live_capture, CoreError};

#[test]
fn default_devices_are_opened_or_reported_as_typed_error() {
    match probe_default_devices() {
        Ok(probe) => {
            assert_eq!(probe.backend, "wasapi");
            assert_eq!(
                probe.devices.len(),
                2,
                "expected microphone and system audio"
            );
            let microphone = &probe.devices[0];
            let system_audio = &probe.devices[1];
            assert_eq!(microphone.channel, "microphone");
            assert_eq!(microphone.direction, "capture");
            assert_eq!(system_audio.channel, "system_audio");
            assert_eq!(system_audio.direction, "render_loopback");
            assert_eq!(microphone.sample_rate, 48_000);
            assert_eq!(microphone.channels, 2);
            assert_eq!(microphone.state, "active");
            assert!(
                !microphone.name.trim().is_empty(),
                "device name must not be empty"
            );
            assert!(
                !system_audio.name.trim().is_empty(),
                "device name must not be empty"
            );
        }
        Err(CoreError::AudioDevice(message)) => {
            assert!(
                !message.trim().is_empty(),
                "device error must carry a message"
            );
        }
        Err(other) => panic!("probe returned an unexpected error: {other}"),
    }
}

#[test]
fn live_capture_streams_frames_or_reports_typed_error() {
    let streams = match start_live_capture() {
        Ok(streams) => streams,
        Err(CoreError::AudioDevice(message)) => {
            assert!(
                !message.trim().is_empty(),
                "device error must carry a message"
            );
            return;
        }
        Err(other) => panic!("live capture returned an unexpected error: {other}"),
    };

    assert_eq!(streams.len(), 2, "expected microphone and system audio");
    let channels: Vec<&str> = streams
        .iter()
        .map(|(device, _)| device.channel.as_str())
        .collect();
    assert!(channels.contains(&"microphone"), "{channels:?}");
    assert!(channels.contains(&"system_audio"), "{channels:?}");

    // The microphone endpoint should advance; a silent system endpoint may not.
    let deadline = Instant::now() + Duration::from_millis(400);
    let mut microphone_frames = 0;
    while Instant::now() < deadline {
        microphone_frames = streams
            .iter()
            .find(|(device, _)| device.channel == "microphone")
            .map_or(0, |(_, stream)| stream.frames());
        if microphone_frames > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(
        microphone_frames > 0,
        "microphone stream never produced frames"
    );

    for (device, stream) in &streams {
        assert!(
            stream.last_error().is_none(),
            "{} reported {}",
            device.channel,
            stream.last_error().unwrap_or_default()
        );
        stream.stop();
    }
}

#[test]
fn device_probe_reports_both_channels_when_hardware_is_present() {
    // On CI without audio devices the typed error is the correct outcome, so a
    // missing device is not a failure; a panic or a wrong shape would be.
    let Ok(probe) = probe_default_devices() else {
        return;
    };
    let channels: Vec<&str> = probe
        .devices
        .iter()
        .map(|device| device.channel.as_str())
        .collect();
    assert!(channels.contains(&"microphone"), "{channels:?}");
    assert!(channels.contains(&"system_audio"), "{channels:?}");
    for device in &probe.devices {
        assert_eq!(device.state, "active");
        assert!(device.sample_rate >= 8_000);
        assert!(device.channels >= 1);
    }
}
