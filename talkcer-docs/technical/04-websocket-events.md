# Talkcer — WebSocket Events

## Client to server
```json
{ "type": "session.subscribe", "session_id": "uuid" }
```

```json
{ "type": "message.send", "session_id": "uuid", "text": "..." }
```

```json
{
  "type": "assistant.action",
  "session_id": "uuid",
  "action": "explain",
  "target": { "kind": "utterance", "id": "utterance_123" }
}
```

```json
{ "type": "capture.start", "session_id": "uuid" }
```

```json
{ "type": "capture.stop", "session_id": "uuid" }
```

```json
{ "type": "overlay.update_preferences", "opacity": 0.9 }
```

## Server to client
```json
{
  "type": "session.state_changed",
  "session_id": "uuid",
  "status": "active"
}
```

```json
{
  "type": "transcript.updated",
  "session_id": "uuid",
  "utterance_id": "uuid",
  "source": "system_audio",
  "speaker": "remote",
  "started_at_ms": 12000,
  "ended_at_ms": 15800,
  "text": "How would you handle retries?",
  "is_final": true
}
```

```json
{
  "type": "assistant.delta",
  "session_id": "uuid",
  "message_id": "uuid",
  "delta": "Idempotency berarti..."
}
```

```json
{
  "type": "assistant.completed",
  "session_id": "uuid",
  "message_id": "uuid"
}
```

```json
{
  "type": "capture.state_changed",
  "session_id": "uuid",
  "state": "capturing",
  "microphone": "active",
  "system_audio": "active"
}
```

```json
{
  "type": "overlay.state_changed",
  "visible": true,
  "always_on_top": true,
  "opacity": 0.9
}
```

```json
{
  "type": "error",
  "session_id": "uuid",
  "code": "STT_PROVIDER_UNAVAILABLE",
  "message": "STT provider is unavailable"
}
```

## Rules
- `utterance_id` tetap sama untuk partial dan final.
- `assistant.delta` hanya berisi incremental text.
- Semua event wajib memiliki `session_id`.
- Event harus idempotent di sisi UI.
