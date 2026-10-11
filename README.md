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

## Windows capture and remote pairing

```http
POST /api/sessions/{id}/capture
POST /api/sessions/{id}/capture/fixture
GET  /api/sessions/{id}/capture/devices
GET  /api/devices/audio
POST /api/remote/pair
```

Capture is Windows-only and opens the real WASAPI endpoints: the default microphone
(`Direction::Capture`) and the default render endpoint in loopback mode
(`Direction::Capture` on a render device), which is how Windows exposes system audio.
`start` returns the opened endpoints under `devices`; `GET .../capture/devices` and
`GET /api/devices/audio` re-open them without changing the lifecycle. A device
failure is a typed `audio device error` (HTTP 503), never a panic.

While capture runs, a background pump drains each reader every 500 ms, splits the
buffer into speech runs with an energy threshold, transcribes each run, and stores
it on the timeline: `microphone` becomes the local speaker and `system_audio` the
remote speaker. New transcripts and a `capture.disconnected` event are published on
the WebSocket. When a reader thread records a read error, the pump stops the readers
and moves capture to `error` automatically; `recover` clears it.

### Speech-to-text selection

Each session picks its own STT provider, so the two channels can run a real model
while another session stays offline. The choice comes from
`session.configuration.stt`, either as a saved configuration id or an inline
provider config. Resolution order is saved id, then inline config, then the
embedded offline adapter.

- `embedded-stt` runs in-process and decodes UTF-8-in-PCM. Deterministic, no network,
  and the default when a session says nothing.
- `self-hosted-stt` and `cloud-stt` post a WAV to `<endpoint>/audio/transcriptions`
  in the OpenAI transcription shape (`file`, `model`, `language`, `response_format`)
  and read `{"text": "..."}`. That covers a local whisper.cpp or faster-whisper
  server and any cloud endpoint that speaks the same protocol.

A remote provider is refused unless the stored config carries `consent: true`, so
audio cannot leave the computer by accident. A cloud provider also needs a
`credentials_ref`; the secret itself is read at call time from the environment
(`TALKCER_SECRET_<NAME>`) and is never stored, logged, or returned to a client. Every
stored transcript records the provider that produced it in `metadata.stt_provider`.

Remote access is off until pairing returns a one-time token. A second computer must
send that bearer token.

Run the capture and STT tests on Windows:

```powershell
cargo test -p talkcer-core --test capture_device
cargo test -p talkcer-core --test capture_pipeline
cargo test -p talkcer-core --test stt_selection
```

## Desktop overlay

```http
POST /api/overlay/show
POST /api/overlay/hide
GET /api/sessions/{id}/export?format=markdown
```

The overlay follows one session and stays empty while hidden. `Ctrl+Shift+Space` is the planned shortcut. This build does not open a Tauri window. A cloud provider needs consent, and a paired remote client stops after 30 requests in one process.
