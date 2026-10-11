# Talkcer — Agent Brief

Talkcer adalah AI Conversation Studio untuk Live Interview, Mock Interview, dan Learning.

Baca seluruh dokumen sebelum implementasi:
- product/01-prd.md
- product/02-prp.md
- product/03-ui-spec.md
- architecture/*.md
- technical/*.md
- delivery/*.md

## Non-negotiable rules
1. Jangan mengubah stack tanpa persetujuan eksplisit.
2. Ikuti API contract, database schema, WebSocket events, dan provider contracts.
3. Jangan membuat fitur di luar task yang diminta.
4. Jika requirement ambigu, ajukan pertanyaan; jangan menebak.
5. Sebelum coding, output daftar file yang akan dibuat atau diubah.
6. Implementasikan satu task pada satu waktu.
7. Tulis test untuk setiap acceptance criteria.
8. Jangan menyimpan API key atau secret di frontend.
9. Jangan memanggil AI provider langsung dari browser.
10. Jangan mengubah migration yang sudah diterapkan; buat migration baru.
11. Overlay adalah fitur desktop opsional dan harus tidak mengganggu aplikasi lain ketika disembunyikan.

## First instruction template
"Implementasikan Task 1 dan Task 2 dari delivery/02-task-breakdown.md. Jangan menambahkan fitur lain. Sertakan test dan cara menjalankannya."
