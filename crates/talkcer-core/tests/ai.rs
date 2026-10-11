use serde_json::json;
use talkcer_core::{
    connect_and_migrate, AiConfigurationService, Capability, ProviderConfig, ProviderKind,
    SaveAiConfiguration, SqliteAiConfigurationRepository,
};
use uuid::Uuid;

fn path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("ai-{name}-{}.sqlite", Uuid::now_v7()))
}

fn slot(kind: ProviderKind, provider: &str, endpoint: Option<&str>, model: &str, secret: Option<&str>) -> ProviderConfig {
    ProviderConfig {
        kind,
        provider: provider.to_owned(),
        endpoint: endpoint.map(str::to_owned),
        model: model.to_owned(),
        credentials_ref: secret.map(str::to_owned),
        consent: kind == ProviderKind::Cloud,
    }
}

fn valid_input() -> SaveAiConfiguration {
    SaveAiConfiguration {
        id: None,
        name: "Default".to_owned(),
        stt: slot(ProviderKind::Embedded, "embedded-stt", None, "local", None),
        thinker: slot(
            ProviderKind::Cloud,
            "cloud-thinker",
            Some("https://llm.example"),
            "talkcer-thinker",
            Some("THINKER_KEY"),
        ),
        tts: slot(
            ProviderKind::SelfHosted,
            "self-hosted-tts",
            Some("http://127.0.0.1:5002"),
            "talkcer-voice",
            None,
        ),
    }
}

#[tokio::test]
async fn configuration_persists_across_restart() {
    let file = path("persist");
    let first = AiConfigurationService::new(SqliteAiConfigurationRepository::new(
        connect_and_migrate(&file).await.unwrap(),
    ));
    let saved = first.save(valid_input()).await.unwrap();
    drop(first);

    let second = AiConfigurationService::new(SqliteAiConfigurationRepository::new(
        connect_and_migrate(&file).await.unwrap(),
    ));
    let listed = second.list().await.unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0], saved);
    assert!(listed[0].stt.credentials_ref.is_none());
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn invalid_configuration_is_rejected() {
    let file = path("invalid");
    let service = AiConfigurationService::new(SqliteAiConfigurationRepository::new(
        connect_and_migrate(&file).await.unwrap(),
    ));
    let mut input = valid_input();
    input.thinker.credentials_ref = Some("sk-live-secret".to_owned());
    let error = service.save(input).await.unwrap_err().to_string();
    assert!(error.contains("credentials_ref"));

    let mut missing = valid_input();
    missing.tts.endpoint = None;
    let error = service.save(missing).await.unwrap_err().to_string();
    assert!(error.contains("endpoint"));
    assert!(service.list().await.unwrap().is_empty());
    let _ = std::fs::remove_file(file);
}

#[tokio::test]
async fn provider_test_reports_capabilities() {
    let file = path("test");
    let service = AiConfigurationService::new(SqliteAiConfigurationRepository::new(
        connect_and_migrate(&file).await.unwrap(),
    ));
    let stt = service
        .providers()
        .iter()
        .find(|provider| provider.provider == "embedded-stt")
        .unwrap();
    assert!(stt.capabilities.supports_partial_transcript);
    assert_eq!(stt.capabilities.max_sample_rate, Some(16_000));

    let saved = service.save(valid_input()).await.unwrap();
    let result = service.test(saved.id, Capability::Thinker).await.unwrap();
    assert!(result.ok);
    assert_eq!(result.provider, "cloud-thinker");
    let _ = json!(result);
    let _ = std::fs::remove_file(file);
}
