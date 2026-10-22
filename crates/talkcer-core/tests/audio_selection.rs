//! Audio selection storage and the per-channel model choice.
//!
//! The selection decides which physical device capture opens and which model
//! reads each channel, so it must survive a restart and must never be silently
//! ignored.

use std::sync::Arc;

use talkcer_core::{
    connect_and_migrate, selection_for_model, AudioSelection, AudioSelectionService,
    ConfiguredSttResolver, ProviderConfig, ProviderKind, SqliteAiConfigurationRepository,
};

fn path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("{name}-{}.sqlite", uuid::Uuid::now_v7()))
}

#[tokio::test]
async fn selection_starts_empty() {
    let file = path("audio-empty");
    let pool = connect_and_migrate(&file).await.unwrap();
    let service = AudioSelectionService::new(pool);
    let saved = service.get().await.unwrap();
    assert_eq!(saved, AudioSelection::default());
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn selection_survives_a_restart() {
    let file = path("audio-restart");
    let pool = connect_and_migrate(&file).await.unwrap();
    let service = AudioSelectionService::new(pool.clone());
    service
        .save(AudioSelection {
            microphone: "  mic-a  ".to_owned(),
            playback: "speakers-b".to_owned(),
            microphone_model: "deepgram:nova-3".to_owned(),
            system_model: String::new(),
        })
        .await
        .unwrap();

    // A fresh service over the same file stands in for a process restart.
    let restarted = AudioSelectionService::new(pool);
    restarted.load().await.unwrap();
    let saved = restarted.get().await.unwrap();
    assert_eq!(saved.microphone, "mic-a", "values are trimmed");
    assert_eq!(saved.playback, "speakers-b");
    assert_eq!(saved.microphone_model, "deepgram:nova-3");
    assert_eq!(saved.system_model, "");

    // The cached value is what capture reads, and it is populated by load.
    assert_eq!(restarted.current().microphone, "mic-a");
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn save_refreshes_the_cache_used_by_capture() {
    let file = path("audio-cache");
    let pool = connect_and_migrate(&file).await.unwrap();
    let service = AudioSelectionService::new(pool);
    assert_eq!(service.current(), AudioSelection::default());
    service
        .save(AudioSelection {
            microphone: "mic-new".to_owned(),
            ..AudioSelection::default()
        })
        .await
        .unwrap();
    assert_eq!(service.current().microphone, "mic-new");
    let _ = std::fs::remove_file(file);
}

#[test]
fn each_channel_reads_its_own_model() {
    let selection = AudioSelection {
        microphone_model: "deepgram:nova-3".to_owned(),
        system_model: "stt-openai:gpt-4o-transcribe".to_owned(),
        ..AudioSelection::default()
    };
    assert_eq!(selection.model_for("microphone"), Some("deepgram:nova-3"));
    assert_eq!(
        selection.model_for("system_audio"),
        Some("stt-openai:gpt-4o-transcribe")
    );
    // An unexpected channel falls back to the microphone value rather than
    // dropping the choice.
    assert_eq!(selection.model_for("other"), Some("deepgram:nova-3"));
}

#[test]
fn blank_channel_model_means_session_default() {
    let selection = AudioSelection {
        microphone_model: "   ".to_owned(),
        ..AudioSelection::default()
    };
    assert_eq!(selection.model_for("microphone"), None);
}

#[test]
fn a_catalog_provider_is_accepted_by_name() {
    // The session picker writes `provider:model` using catalog ids, so these
    // must validate rather than fail as unknown providers.
    let selection = selection_for_model("stt-openai:gpt-4o-transcribe", &[])
        .expect_err("a cloud provider with no saved configuration is refused");
    assert!(selection.to_string().contains("saved configuration"));
}

#[test]
fn a_local_catalog_provider_needs_no_saved_configuration() {
    let selection = selection_for_model("whisper:tiny", &[]).expect("local model resolves");
    assert_eq!(selection.kind, ProviderKind::Embedded);
    assert_eq!(selection.model, "tiny");
}

#[test]
fn a_value_without_a_colon_is_rejected() {
    let error = selection_for_model("whisper", &[]).unwrap_err();
    assert!(error.to_string().contains("provider:model"));
}

#[tokio::test]
async fn resolver_uses_the_saved_channel_model() {
    let file = path("audio-resolver");
    let pool = connect_and_migrate(&file).await.unwrap();
    let audio = AudioSelectionService::new(pool.clone());
    audio
        .save(AudioSelection {
            microphone_model: "whisper:tiny".to_owned(),
            system_model: "whisper:base".to_owned(),
            ..AudioSelection::default()
        })
        .await
        .unwrap();
    let resolver = ConfiguredSttResolver::new(Arc::new(SqliteAiConfigurationRepository::new(
        pool,
    )))
    .with_audio_selection(Arc::new(audio));

    assert_eq!(
        resolver.channel_model("microphone").await.unwrap(),
        Some("whisper:tiny".to_owned())
    );
    assert_eq!(
        resolver.channel_model("system_audio").await.unwrap(),
        Some("whisper:base".to_owned())
    );
    let _ = std::fs::remove_file(file);
}

#[test]
fn saved_slot_names_still_validate() {
    // The legacy slot provider names must keep working for older rows.
    let config = ProviderConfig {
        kind: ProviderKind::Embedded,
        provider: "embedded-stt".to_owned(),
        endpoint: None,
        model: "embedded-utf8".to_owned(),
        credentials_ref: None,
        consent: false,
    };
    assert!(talkcer_core::selection_from_config(&config).is_ok());
}
