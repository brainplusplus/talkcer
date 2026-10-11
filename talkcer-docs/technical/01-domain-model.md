# Talkcer — Domain Model

## Session
```rust
enum SessionMode { LiveInterview, MockInterview, Learning }
enum SessionStatus { Ready, Active, Paused, Completed, Archived }

struct Session {
  id: Uuid,
  title: String,
  mode: SessionMode,
  status: SessionStatus,
  context: serde_json::Value,
  configuration: serde_json::Value,
  created_at: DateTime<Utc>,
  updated_at: DateTime<Utc>,
}
```

## TimelineItem
```rust
enum ItemKind { Transcript, UserMessage, AiMessage, Feedback, SystemEvent }
enum Speaker { LocalUser, Remote, Ai, System }
enum Source { Microphone, SystemAudio, Text, AiGenerated }
enum Purpose { Conversation, AssistantInstruction, AssistantResponse }

struct TimelineItem {
  id: Uuid,
  session_id: Uuid,
  kind: ItemKind,
  speaker: Speaker,
  source: Source,
  purpose: Purpose,
  text: String,
  started_at_ms: i64,
  ended_at_ms: Option<i64>,
  is_final: bool,
  metadata: serde_json::Value,
}
```

## AI configuration
```rust
enum ProviderKind { Embedded, SelfHosted, Cloud }

struct ProviderConfig {
  kind: ProviderKind,
  endpoint: Option<String>,
  model: String,
  credentials_ref: Option<String>,
}

struct AiConfiguration {
  stt: ProviderConfig,
  thinker: ProviderConfig,
  tts: ProviderConfig,
}
```

## Overlay preferences
```rust
struct OverlayPreferences {
  enabled: bool,
  always_on_top: bool,
  opacity: f32,
  width: u32,
  height: u32,
  position_x: i32,
  position_y: i32,
  click_through_non_interactive_area: bool,
  show_remote_transcript: bool,
  show_local_transcript: bool,
  show_ai_response: bool,
}
```
