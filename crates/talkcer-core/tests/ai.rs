use serde_json::json;
use std::collections::BTreeMap;

use talkcer_core::{
    connect_and_migrate, thinker_from_config, AiConfigurationService, Capability, OfferedModel,
    ProviderConfig, ProviderKind, ProviderSelection, SaveAiConfiguration, SelectionService,
    SqliteAiConfigurationRepository,
};
use uuid::Uuid;

fn path(name: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join("talkcer-tests");
    std::fs::create_dir_all(&dir).unwrap();
    dir.join(format!("ai-{name}-{}.sqlite", Uuid::now_v7()))
}

fn slot(
    kind: ProviderKind,
    provider: &str,
    endpoint: Option<&str>,
    model: &str,
    secret: Option<&str>,
) -> ProviderConfig {
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
        stt: slot(
            ProviderKind::Embedded,
            "embedded-stt",
            None,
            "embedded",
            None,
        ),
        thinker: slot(
            ProviderKind::Cloud,
            "openai",
            Some("https://api.openai.com/v1"),
            "gpt-4o-mini",
            Some("THINKER_KEY"),
        ),
        tts: slot(
            ProviderKind::SelfHosted,
            "piper",
            Some("http://127.0.0.1:5002"),
            "en_US-lessac-medium",
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
    missing.thinker.endpoint = None;
    let error = service.save(missing).await.unwrap_err().to_string();
    assert!(error.contains("endpoint"), "{error}");
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
    assert_eq!(result.provider, "openai");
    let _ = json!(result);
    let _ = std::fs::remove_file(file);
}

#[test]
fn named_thinker_providers_resolve_and_unknown_names_do_not() {
    let ollama = ProviderConfig {
        kind: ProviderKind::SelfHosted,
        provider: "ollama".to_owned(),
        endpoint: Some("http://127.0.0.1:11434/v1".to_owned()),
        model: "llama3.2".to_owned(),
        credentials_ref: None,
        consent: true,
    };
    let selection = thinker_from_config(&ollama).unwrap();
    assert_eq!(selection.provider, "ollama");
    assert_eq!(selection.model, "llama3.2");
    assert_eq!(selection.kind, ProviderKind::SelfHosted);

    let unknown = ProviderConfig {
        kind: ProviderKind::Embedded,
        provider: "nope".to_owned(),
        endpoint: None,
        model: "x".to_owned(),
        credentials_ref: None,
        consent: false,
    };
    let error = thinker_from_config(&unknown).unwrap_err();
    assert!(matches!(error, talkcer_core::CoreError::UnknownProvider(name) if name == "nope"));
}

#[tokio::test]
async fn provider_selection_round_trips() {
    let file = path("selection");
    let service = SelectionService::new(connect_and_migrate(&file).await.unwrap());
    let empty = service.get("tts").await.unwrap();
    assert!(empty.offered.is_empty());
    assert!(empty.disabled.is_empty());
    assert!(empty.defaults.is_empty());

    let mut defaults = BTreeMap::new();
    defaults.insert("ollama".to_owned(), "llama3.2".to_owned());
    let saved = service
        .save(ProviderSelection {
            capability: "thinker".to_owned(),
            offered: vec![
                OfferedModel {
                    provider: " ollama ".to_owned(),
                    model: " llama3.2 ".to_owned(),
                },
                OfferedModel {
                    provider: "ollama".to_owned(),
                    model: "llama3.2".to_owned(),
                },
                OfferedModel {
                    provider: " ".to_owned(),
                    model: "dropped".to_owned(),
                },
            ],
            disabled: vec![" groq ".to_owned(), "groq".to_owned(), " ".to_owned()],
            defaults,
        })
        .await
        .unwrap();
    assert_eq!(saved.offered.len(), 1);
    assert_eq!(saved.offered[0].provider, "ollama");
    assert_eq!(saved.offered[0].model, "llama3.2");
    assert_eq!(saved.disabled, vec!["groq".to_owned()]);
    assert_eq!(
        saved.defaults.get("ollama").map(String::as_str),
        Some("llama3.2")
    );

    let loaded = service.get("thinker").await.unwrap();
    assert_eq!(loaded, saved);
    let _ = std::fs::remove_file(file);
}
