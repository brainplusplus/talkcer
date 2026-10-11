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

## WebSocket
```http
WS /ws?session_id=<uuid>
```
