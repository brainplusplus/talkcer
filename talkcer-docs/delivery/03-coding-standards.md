# Talkcer — Coding Standards

## Rust
- Gunakan workspace dengan crate terpisah.
- Error menggunakan `thiserror` untuk library dan `anyhow` untuk application boundary.
- Async menggunakan Tokio.
- Jangan melakukan blocking I/O di async task.
- Gunakan `tracing` untuk logging.
- Semua public API memiliki doc comment.
- Test unit untuk domain logic dan state machine.

## TypeScript
- Strict mode.
- TanStack Query untuk server state.
- Zustand atau Context hanya untuk UI state.
- Komponen kecil dan presentational.
- WebSocket client harus punya reconnect dan backoff.

## Overlay
- Overlay adalah client, bukan capture engine.
- Jangan menjalankan audio capture di dalam overlay.
- Simpan posisi dan preferensi overlay.
- Overlay harus responsif terhadap session switch.

## General
- Jangan menaruh secret di frontend.
- Jangan memanggil AI provider langsung dari UI.
- Semua komunikasi AI melalui core API.
- Setiap fitur harus punya test.
- Setiap task harus dapat dijalankan dan diverifikasi.
