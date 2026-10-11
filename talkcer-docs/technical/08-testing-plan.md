# Talkcer — Testing Plan

## Unit tests
- Session CRUD.
- Timeline ordering.
- Partial-to-final transcript update.
- AI configuration validation.
- State machine transitions.
- Overlay preferences validation.

## Integration tests
- WebSocket subscribe dan event delivery.
- Create session -> send message -> assistant response.
- Capture start/stop lifecycle.
- Provider mock failure dan retry.
- Overlay subscribe ke session aktif.

## E2E tests
- Buat mock interview, jawab dengan teks, terima feedback.
- Buat learning session, kirim pertanyaan, terima jawaban streaming.
- Buat live session, inject audio sample, verifikasi transcript.
- Remote client membuka session dan mengirim action.
- Overlay muncul, menampilkan transcript, lalu disembunyikan.

## Critical failure cases
- Stop session saat AI streaming.
- Start session baru saat request lama masih berjalan.
- Mic disconnect saat capture aktif.
- STT provider timeout.
- WebSocket reconnect.
- Cloud provider gagal saat local dipilih.
- Partial transcript berubah sebelum final.
- Overlay tertutup saat session berganti.
