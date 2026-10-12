# Talkcer

AI Conversation Studio. This repository currently contains Task 1 and Task 2 only.

## Run

```powershell
cargo run --bin talkcer-cli serve
```

The API and the web app share one port: `http://127.0.0.1:54000`. The server serves
the built web bundle from `apps/web/dist` on that same port with an `index.html`
fallback, so a client-side route such as `/live/new` loads the app.

```powershell
cd apps/web
npm install
npm run build
```

In development the Vite dev server runs on `http://127.0.0.1:54001` and proxies
`/api` and `/ws` to port 54000.

```powershell
cd apps/web
npm run dev
```

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
curl.exe http://127.0.0.1:54000/api/health
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

Configuration stores a credential reference, never a raw key. The mock route runs on
the session's configured thinker; with no configuration it uses the embedded offline
templates, so it stays usable without a model.

## Learning and browser voice

```http
POST /api/sessions/{id}/learning
POST /api/sessions/{id}/voice/audio
POST /api/sessions/{id}/voice/speak
```

A learning session stays active and can continue after it is reopened. Tests use mock audio. Browser capture is marked `browser_mic` in metadata. Playback only works when session TTS is enabled, and the voice comes from the session's configured TTS provider, falling back to the embedded tone generator.

## AI providers (thinker and speech)

Every AI capability follows the same seam as speech-to-text: a session points at a
provider, resolution order is saved configuration id, then inline config, then the
embedded offline adapter, and a remote provider is refused unless it carries
`consent: true`.

### Thinker (language model)

Text for assistant actions, mock interviews, and learning turns comes from the
session's thinker (`session.configuration.thinker`).

- `embedded-thinker` reproduces the original deterministic local templates. No
  network, and the default when a session says nothing.
- `self-hosted-thinker` and `cloud-thinker` post an OpenAI-compatible
  `/chat/completions` request (`model`, `messages`) and read
  `choices[0].message.content`. That covers a local Ollama or vLLM server and any
  cloud endpoint that speaks the same protocol.

Every stored AI row records `metadata.provider` and `metadata.task`, and sets
`metadata.stub` when the embedded adapter answered.

### Text-to-speech

TTS stays gated by `session.configuration.tts == "enabled"`. The provider is chosen
separately from `session.configuration.tts_provider`, so turning speech on and
picking where it runs stay independent.

- `embedded-tts` (and `disabled-tts`) return a local tone WAV. Deterministic and
  network-free.
- `self-hosted-tts` and `cloud-tts` post to `<endpoint>/audio/speech`
  (`model`, `input`, `voice`, `response_format`) and return audio bytes.

Both remote thinker and remote TTS need `consent: true`, and a cloud provider also
needs a `credentials_ref`. The secret is read at call time from the environment
(`TALKCER_SECRET_<NAME>`) and is never stored, logged, or returned.

Run the AI provider tests:

```powershell
cargo test -p talkcer-core --test thinker_selection
cargo test -p talkcer-core --test thinker_pipeline
cargo test -p talkcer-core --test tts_selection
cargo test -p talkcer-server --test ai_pipeline_api
```

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

The overlay follows one session and stays empty while hidden. `Ctrl+3` toggles it from anywhere. A cloud provider needs consent, and a paired remote client stops after 30 requests in one process.

## Desktop shell (Tauri)

The native shell is real and builds on Windows with the `tauri-shell` feature. It is
off by default so the workspace stays headless and CI never needs the WebView2
toolchain.

```powershell
# terminal 1: the core API and the web bundle on one port
cargo run --bin talkcer-cli serve

# terminal 2: the native shell (main window + overlay + tray)
cargo run -p talkcer-desktop --features tauri-shell --bin talkcer-desktop
```

The main window loads the web workspace from `http://127.0.0.1:54000/`. Inside the
shell the web app resolves its API and WebSocket origin to the same port (set
`VITE_TALKCER_API` to override), and `withGlobalTauri` exposes
`window.__TAURI__.core.invoke` so the overlay page can ask the shell to move,
hide, and act on the native window.

The shell:

- opens the web workspace in the main window;
- keeps a compact, transparent, undecorated overlay as a second window that loads
  `/overlay-window`, skips the taskbar, and starts hidden;
- registers five global shortcuts, each independent of window focus. A busy combo
  fails loudly on stderr but never kills the shell; the tray stays as fallback:
  - `Ctrl+1` shows or hides the whole application;
  - `Ctrl+2` toggles undetectable mode: every window is excluded from screen
    capture (`WDA_EXCLUDEFROMCAPTURE`, re-asserted every 500 ms because the OS
    can drop it), the taskbar button disappears, and the window title becomes a
    neutral system name. Toggling off restores all three;
  - `Ctrl+3` toggles the overlay window;
  - `Ctrl+4` opens the settings page;
  - `Ctrl+5` opens the currently running session (its mode and detail), or home
    when nothing is running;
- keeps the overlay reachable from a tray menu (toggle/show/hide, open workspace,
  quit);
- hides the overlay instead of quitting when its window is closed;
- polls saved preferences every 500 ms and re-applies the idempotent window plan,
  so changes made from the web workspace (or any API client) reach the native
  window;
- never captures audio and never talks to a provider directly: quick actions
  (Explain, Translate, Suggest, Hide) go through the core API.

### Manual verification (Windows, WebView2 present)

1. Build the web bundle first: `cd apps/web; npm install; npm run build`.
2. Start the server and the shell as above. The main window titled *Talkcer*
   opens and loads the workspace.
3. Toggle the overlay: `curl.exe -X POST http://127.0.0.1:54000/api/overlay/toggle`.
   Within half a second the overlay window appears above other windows.
4. Move it: `curl.exe -X POST -H "content-type: application/json" -d "{\"x\":300,\"y\":400}" http://127.0.0.1:54000/api/overlay/move`.
   The window moves to exactly (300,400).
5. Press `Ctrl+1` through `Ctrl+5` with no Talkcer window focused: hide and show
   the app, toggle undetectable mode, toggle the overlay, open settings, and open
   the running session. A combo owned by another application prints
   `shortcut Ctrl+N unavailable` and the rest still work.
6. Close the overlay window: it hides; the workspace and the tray stay alive.
7. Quit from the tray to end the process.

One session runs at a time. `GET /api/sessions/current` returns the `active` or
`paused` session, and activating one session pauses any other active session. The
session page offers Start, Stop, and Resume.

Verified in this build: all five shortcuts register on a clean machine, the main
window opens with title *Talkcer*, the overlay window is shown and moved to the
exact requested coordinates by the sync loop, client-side routes serve as 200 from
the API port, and all 120 workspace tests plus the web suite pass.
