# ADR-003: Tauri Desktop Shell & Overlay

## Status
Accepted

## Context
Butuh desktop app, tray, global shortcut, dan overlay untuk satu PC.

## Decision
Gunakan Tauri sebagai desktop shell. Overlay adalah secondary Tauri window.

## Overlay window requirements
- `always_on_top`: true ketika overlay aktif.
- `transparent`: true atau translucent background.
- `decorations`: false untuk custom compact UI.
- `ignore_cursor_events`: dapat diaktifkan untuk area non-interaktif.
- Bisa disembunyikan dan dipindah.
- Tidak boleh fullscreen secara default.

## References
Tauri mendukung `setAlwaysOnTop`, transparent window, dan `setIgnoreCursorEvents`. [web:59][web:60][web:62]

## Consequences
- Overlay dapat menampilkan transcript dan AI response di atas aplikasi lain.
- Click-through perlu dikelola hati-hati agar tidak memblokir aplikasi interview.
- Overlay harus punya escape yang jelas: hide, quit overlay, atau disable.
