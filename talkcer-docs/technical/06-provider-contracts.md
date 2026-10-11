# Talkcer — Provider Contracts

## STT
```rust
#[async_trait]
pub trait SttProvider: Send + Sync {
    async fn start(&self, config: SttConfig) -> Result<SttStream, ProviderError>;
}

pub trait SttStream: Send {
    fn send_audio(&mut self, chunk: AudioChunk) -> Result<(), ProviderError>;
    fn next_event(&mut self) -> impl Future<Output = Option<SttEvent>> + Send;
    fn finish(&mut self) -> Result<(), ProviderError>;
}

pub enum SttEvent {
    Partial { utterance_id: Uuid, text: String },
    Final { utterance_id: Uuid, text: String, started_at_ms: i64, ended_at_ms: i64 },
    Error(ProviderError),
}
```

## Thinker / LLM
```rust
#[async_trait]
pub trait ThinkerProvider: Send + Sync {
    async fn complete(&self, request: ThinkerRequest)
        -> Result<ThinkerStream, ProviderError>;
}

pub enum ThinkerEvent {
    Delta(String),
    Completed(ThinkerResult),
    Error(ProviderError),
}
```

## TTS
```rust
#[async_trait]
pub trait TtsProvider: Send + Sync {
    async fn synthesize(&self, request: TtsRequest)
        -> Result<TtsAudio, ProviderError>;
}
```

## Provider capabilities
- `supports_streaming`
- `supports_partial_transcript`
- `supports_word_timestamps`
- `supports_cancellation`
- `max_sample_rate`
- `supported_languages`

## Error taxonomy
- `ProviderUnavailable`
- `AuthenticationFailed`
- `RateLimited`
- `InvalidConfiguration`
- `AudioFormatUnsupported`
- `StreamDisconnected`
- `Timeout`
- `Cancelled`
