# Talkcer — Task Breakdown

## Task 1 — Project skeleton
Create:
- Rust workspace: `talkcer-core`, `talkcer-server`, `talkcer-cli`, `talkcer-desktop`.
- React + TypeScript web app.
- SQLite migrations.
- Axum server.
- `GET /api/health`.

Acceptance:
- `cargo run --bin talkcer-cli serve` starts server.
- `GET /api/health` returns 200.
- Web app loads.

## Task 2 — Session CRUD
Implement:
- Create, list, get, update, delete sessions.
- Filter by mode and status.
- SQLite persistence.

Acceptance:
- Session persists after restart.
- Invalid mode rejected.
- Sessions grouped by mode.

## Task 3 — Timeline
Implement:
- Append timeline item.
- Update partial transcript.
- Finalize transcript.
- WebSocket streaming.

Acceptance:
- Partial update does not create new bubble.
- Final update replaces partial.
- Events contain session_id and utterance_id.

## Task 4 — AI configuration
Implement:
- Provider registry.
- STT, thinker, TTS configuration.
- Provider test endpoint.

Acceptance:
- Configuration persists.
- Invalid provider config rejected.
- Provider capability exposed to UI.

## Task 5 — Mock interview
Implement:
- Mock controller.
- Question generation.
- Answer handling.
- Follow-up.
- Feedback.

Acceptance:
- User can complete a mock interview using text.
- Transcript and AI messages stored in timeline.

## Task 6 — Learning
Implement:
- Learning controller.
- Topic context.
- Practice questions.
- Summary.

Acceptance:
- User can continue a learning session.
- Session history retained.

## Task 7 — Browser voice
Implement:
- Browser mic capture.
- Streaming STT adapter.
- Partial/final transcript.
- TTS playback.

Acceptance:
- User can speak and see transcript.
- AI can speak response if TTS enabled.

## Task 8 — Live native capture
Implement:
- Native mic capture.
- System audio capture untuk satu platform target.
- Dual STT pipeline.
- Live controller.

Acceptance:
- Remote and local transcript appear separately.
- Capture can start, pause, stop, and recover.

## Task 9 — Remote control
Implement:
- Remote auth.
- Remote session list.
- Remote timeline.
- Remote assistant action.

Acceptance:
- Browser dari PC kedua dapat mengontrol session.
- Remote access tidak aktif tanpa pairing.

## Task 10 — Desktop & overlay
Implement:
- Tauri main window.
- Tray.
- Global shortcut.
- Overlay secondary window.
- Overlay preferences.
- Overlay quick actions.

Acceptance:
- Desktop membuka web UI.
- Overlay dapat show, hide, move, dan adjust opacity.
- Overlay menampilkan transcript dan AI response session aktif.
- Overlay tidak menghalangi aplikasi ketika disembunyikan.
- Shortcut overlay berfungsi saat window tidak fokus.
