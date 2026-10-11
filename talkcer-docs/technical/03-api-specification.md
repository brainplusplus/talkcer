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
POST /config/ai/test
```

## Audio devices
```http
GET /devices/audio
```

## Live capture
```http
POST /sessions/:id/capture/start
POST /sessions/:id/capture/stop
POST /sessions/:id/capture/pause
POST /sessions/:id/capture/resume
```

## Overlay
```http
GET /settings/overlay
PUT /settings/overlay
POST /overlay/show
POST /overlay/hide
POST /overlay/toggle
```

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
