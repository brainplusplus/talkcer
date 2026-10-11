# Talkcer — System Architecture

## High-level
```text
Browser / Desktop UI / Overlay
              |
              | HTTP + WebSocket
              v
       Rust Core API
              |
   Application services
              |
Audio capture + AI providers
```

## Components
- `talkcer-core`: domain, services, storage, provider abstraction.
- `talkcer-server`: Axum HTTP + WebSocket.
- `talkcer-cli`: CLI.
- `talkcer-desktop`: Tauri wrapper.
- `talkcer-web`: React UI.
- `talkcer-overlay`: Tauri secondary window.

## Runtime model MVP
- Core dan server berjalan dalam satu proses.
- Desktop Tauri dapat menjalankan atau terhubung ke core.
- Remote browser terhubung ke core API.
- Browser mic digunakan untuk mock/learning awal.
- Native mic dan system audio digunakan untuk live interview.
- Overlay adalah client tambahan dari core API.

## Data flow: live interview
```text
System audio -> STT -> transcript remote -> timeline -> workspace / overlay
Mic          -> STT -> transcript local  -> timeline -> workspace / overlay
User chat / action ----------------------> thinker -> AI response -> timeline
```

## Data flow: mock / learning
```text
User mic -> STT -> transcript -> thinker -> response -> timeline
User text ------------------------------> thinker -> response -> timeline
AI response -> optional TTS -> playback
```

## Overlay architecture
```text
Rust core API
      |
      +--> Main workspace window
      +--> Overlay window
      +--> Remote browser
```

Overlay bukan proses capture terpisah. Ia hanya subscription client terhadap session aktif.

## Storage
- SQLite untuk session, timeline, konfigurasi, attachment metadata.
- File storage lokal untuk attachment.
- Audio mentah opsional dan terpisah dari transcript.

## Security
- API default bind `127.0.0.1`.
- Remote access memerlukan pairing token dan auth.
- Credential disimpan terenkripsi.
- Cloud usage harus menampilkan indikator dan consent.
