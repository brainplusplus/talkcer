# Talkcer — Product / Project Plan

## 1. Principles
1. Session-first.
2. Timeline-first.
3. Web-first UI.
4. Native only where required.
5. Provider-agnostic AI.
6. Explicit cloud consent.
7. Manual assistance before automatic assistance.
8. Overlay harus membantu, bukan mengganggu.

## 2. Architecture overview
- Web UI: React + TypeScript.
- Core: Rust, Axum, SQLite.
- Desktop: Tauri wrapper.
- CLI: Rust Clap.
- Remote: Browser mengakses core API.
- AI: pluggable STT, LLM, TTS.
- Overlay: Tauri secondary window.

## 3. Delivery phases
### Phase 0 — Foundation (1 week)
- Repository, CI, lint, test.
- API contract.
- Data model.
- Project skeleton.

### Phase 1 — Web workspace (2-3 weeks)
- Sidebar kategori.
- New session.
- History.
- Timeline.
- Chat.
- AI configuration.
- SQLite persistence.

### Phase 2 — Voice mock & learning (2 weeks)
- Browser mic.
- Streaming STT.
- TTS playback.
- Mock controller.
- Learning controller.

### Phase 3 — Live core (3-4 weeks)
- Rust mic capture.
- System audio capture untuk platform target pertama.
- Dual STT pipeline.
- Live controller.

### Phase 4 — Remote control (1-2 weeks)
- Remote auth.
- Pairing.
- Remote session list.
- Remote timeline dan action.

### Phase 5 — Desktop & overlay (2 weeks)
- Tauri wrapper.
- Tray.
- Global shortcut.
- Overlay window.
- Overlay settings.

### Phase 6 — Hardening (2 weeks)
- Error handling.
- Reconnect.
- Device change.
- Provider timeout.
- Export.
- Security audit.

## 4. Definition of done
Lihat PRD section MVP definition of done.
