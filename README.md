# Talkcer

AI Conversation Studio. This repository currently contains Task 1 and Task 2 only.

## Run

```powershell
cargo run --bin talkcer-cli serve
```

The API listens on `http://127.0.0.1:8787`.

```powershell
cd apps/web
npm install
npm run dev
```

Open `http://127.0.0.1:5173`. The dev server proxies `/api` to the Rust server.

## Verify

```powershell
cargo test --workspace
```

```powershell
cd apps/web
npm test
npm run build
```

Health check:

```powershell
curl.exe http://127.0.0.1:8787/api/health
```

## Timeline

Partial and final transcripts:

```http
POST /api/sessions/{id}/transcripts
GET /api/sessions/{id}/timeline
WS /ws?session_id={id}
```

A repeated `utterance_id` updates the same row. A final row rejects later updates with `409`. `assistant.action` stores a local stub and does not call a provider.

## AI configuration and mock interview

```http
GET /api/config/ai/providers
PUT /api/config/ai
POST /api/config/ai/{id}/test
POST /api/sessions/{id}/mock
```

Configuration stores a credential reference, never a raw key. The mock route is text-only and does not call a model.

## Learning and browser voice

```http
POST /api/sessions/{id}/learning
POST /api/sessions/{id}/voice/audio
POST /api/sessions/{id}/voice/speak
```

A learning session stays active and can continue after it is reopened. Tests use mock audio. Browser capture is marked `browser_mic` in metadata. Playback is a local WAV and only works when session TTS is enabled.
