# Talkcer — API Specification

Base URL: `http://127.0.0.1:8787/api`

## Health
```http
GET /health
200 { "status": "ok", "app": "talkcer", "version": "0.1.0" }
```

## Sessions
```http
GET /sessions?mode=live_interview&status=active
POST /sessions
GET /sessions/:id
PATCH /sessions/:id
DELETE /sessions/:id
```

### Create session
```json
{
  "title": "Backend Engineer — Acme",
  "mode": "live_interview",
  "context": {
    "role": "Senior Backend Engineer",
    "company": "Acme",
    "language": "en"
  },
  "configuration": {
    "stt": "default",
    "thinker": "default",
    "tts": "disabled"
  }
}
```

## Messages
```http
POST /sessions/:id/messages
```
```json
{
  "kind": "user_message",
  "text": "Jelaskan pertanyaan terakhir.",
  "metadata": {}
}
```

## Assistant actions
```http
POST /sessions/:id/assistant-actions
```
```json
{
  "action": "explain",
  "target": { "kind": "utterance", "id": "utterance_123" },
  "language": "id"
}
```

Supported actions:
- `explain`
- `translate`
- `summarize`
- `suggest_points`
- `custom_instruction`

## AI configuration
```http
GET /config/ai
PUT /config/ai
GET /config/ai/providers
POST /config/ai/:id/test
```

`PUT` stores `embedded`, `self_hosted`, or `cloud` slots. Cloud requires `endpoint` and `credentials_ref`. A raw secret is rejected. `GET /config/ai/providers` exposes capability flags to the UI. The test route checks the saved configuration against the registry and does not call a remote provider.

## Mock interview
```http
POST /sessions/:id/mock
```

```json
{ "action": "start" }
{ "action": "answer", "text": "I separated retries from the request path." }
{ "action": "complete", "text": "I would keep retries bounded." }
```

Text-only. `start` stores the first question and sets `active`. `answer` stores the answer and a follow-up. `complete` stores feedback and a summary, then sets `completed`. The generated rows remain on `GET /sessions/:id/timeline`.

## Learning
```http
POST /sessions/:id/learning
```

```json
{ "action": "start" }
{ "action": "discuss", "text": "When should I add an index?" }
{ "action": "answer", "text": "When the filter is selective." }
```

Text-only. `start` introduces `context.topic` and stores a practice prompt. The session stays `active`, so a later `discuss` continues the same timeline after a restart.

## Browser voice
```http
POST /sessions/:id/voice/audio
POST /sessions/:id/voice/speak
```

`voice/audio` accepts `audio_source` `browser_mic` or `mock_audio`. The mock adapter uses `text` or base64 UTF-8 chunks as the transcript. The same `utterance_id` updates one row, and the first `browser_mic` value stays in metadata. `voice/speak` returns a local WAV only when `session.configuration.tts` is `"enabled"`. No remote STT or TTS provider is called.

## Audio devices
```http
GET /devices/audio
```

## Live capture
Windows only. Tests inject fixtures instead of opening WASAPI.

```http
GET /sessions/:id/capture
POST /sessions/:id/capture
POST /sessions/:id/capture/fixture
```

`POST /capture` accepts `start`, `pause`, `resume`, `stop`, `disconnect`, and `recover`. Fixtures are accepted only while capturing. `microphone` stays the local speaker and `system_audio` stays the remote speaker.

## Remote access
```http
GET /remote/status
POST /remote/pair
```

Pairing is off by default. Enabling it returns a token once; only its SHA-256 hash is stored. A second computer sends `x-talkcer-client: remote` and `Authorization: Bearer <token>`. A non-loopback connection must present that token too. Disabling pairing rejects the old token. The server still binds to `127.0.0.1` by default. A paired client can make 30 remote requests per process. The next request returns `429`.

## Overlay
```http
GET /overlay
PUT /overlay
POST /overlay/show
POST /overlay/hide
POST /overlay/toggle
POST /overlay/move
POST /overlay/opacity
POST /overlay/click-through
GET /overlay/view
GET /sessions/:id/export?format=json
GET /sessions/:id/export?format=markdown
```

The overlay starts hidden. Show keeps it above other windows. Hide removes it. Move, opacity, and click-through work only while it is visible. Opacity must be from 0.2 through 1. The view contains one session, and switching sessions removes the previous session. Preferences persist in SQLite. Export returns readable JSON or Markdown. A cloud provider also requires `consent: true` before it can be saved.

## Transcripts
Approved for Task 3. A transcript is not a chat message.

```http
POST /sessions/:id/transcripts
```

```json
{
  "utterance_id": "optional-uuid",
  "speaker": "remote",
  "source": "system_audio",
  "text": "How would you handle retries?",
  "started_at_ms": 12000,
  "ended_at_ms": 15800,
  "is_final": false
}
```

- `utterance_id` may be omitted. The server creates one.
- A later partial uses the same `utterance_id` and updates the same row.
- `is_final: true` replaces `text` and closes the utterance.
- A later update returns `409 CONFLICT`.
- `speaker`: `local_user`, `remote`, `ai`, `system`.
- `source`: `microphone`, `system_audio`, `text`, `ai_generated`.
- Success returns `201` for a new row and `200` for an update:
  `{ "utterance_id", "created", "item" }`.
- The server then broadcasts `transcript.updated`.

## Timeline history
```http
GET /sessions/:id/timeline?limit=100&before=<timeline_item_id>
```

- Items are ordered by `started_at_ms`, then `created_at`.
- `limit` defaults to 100 and must be from 1 to 500.
- `before` returns rows older than that item.
- This does not change `GET /sessions/:id`.

## Assistant actions
`POST /sessions/:id/assistant-actions` stores a local stub in Task 3.
It does not call a provider. The response contains the instruction row and a separate `ai_message`.
The target utterance id is stored in metadata. The socket then emits one `assistant.delta` and `assistant.completed`.

## WebSocket
```http
WS /ws?session_id=<uuid>
```
