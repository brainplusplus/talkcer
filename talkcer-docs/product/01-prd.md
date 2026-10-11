# Talkcer — PRD

## 1. Product summary
Talkcer adalah AI Conversation Studio untuk persiapan interview, pendampingan live interview, dan pembelajaran topik tertentu.

Produk berbasis **session**, mirip ChatGPT: pengguna dapat membuat session baru, melanjutkan session lama, melihat history, serta mengombinasikan input suara real-time dan teks/chat dalam satu timeline.

## 2. Modes
| Mode | Lawan bicara | Input utama | Peran AI |
|---|---|---|---|
| Live Interview | Manusia melalui Zoom, Google Meet, dll. | System audio + mic + chat | Memahami percakapan, translate, explain, suggest |
| Mock Interview | AI interviewer | Mic + chat | Bertanya, follow-up, feedback |
| Learning | AI tutor | Mic + chat | Menjelaskan, berdiskusi, latihan |

## 3. Core value proposition
- Satu workspace untuk live interview, mock interview, dan learning.
- Session history per kategori.
- Timeline percakapan yang memisahkan transcript, chat, dan respons AI.
- Input suara maupun teks dalam session yang sama.
- STT, thinker/LLM, dan TTS dapat dikonfigurasi terpisah: embedded, self-hosted, atau cloud.
- Desktop overlay untuk pengguna satu PC.
- Remote control dari browser di PC kedua.

## 4. User journeys
### Live interview
1. User membuat Live Interview session.
2. User memilih mic dan system audio.
3. Core menangkap audio dan melakukan streaming STT.
4. Timeline menampilkan transcript remote dan local.
5. User mengirim aksi AI: explain, translate, suggest.
6. AI response muncul sebagai pesan terpisah.
7. User dapat membuka overlay compact untuk melihat bantuan tanpa pindah aplikasi.

### Mock interview
1. User membuat Mock Interview session.
2. User memilih topik, level, durasi, dan interaction mode.
3. AI mengajukan pertanyaan melalui teks dan opsional TTS.
4. User menjawab melalui mic atau teks.
5. AI memberi follow-up dan feedback.

### Learning
1. User membuat Learning session.
2. User memasukkan topik, tujuan, level, dan referensi.
3. User berdiskusi melalui teks atau mic.
4. AI menjelaskan, memberi contoh, dan latihan.

## 5. Functional requirements
- FR-1 Session CRUD, status, search, archive.
- FR-2 Kategori: Live Interview, Mock Interview, Learning.
- FR-3 Unified timeline dengan partial/final transcript.
- FR-4 Input suara: browser mic, native mic, system audio.
- FR-5 Input teks dan assistant action.
- FR-6 STT, thinker/LLM, TTS dapat dikonfigurasi terpisah.
- FR-7 Live controller: dual-channel capture dan transcript.
- FR-8 Mock controller: AI interviewer, follow-up, feedback.
- FR-9 Learning controller: tutor, latihan, ringkasan.
- FR-10 Remote web control dari PC kedua.
- FR-11 Desktop Tauri wrapper, tray, global shortcut.
- FR-12 Desktop overlay mode untuk satu PC.
- FR-13 CLI: serve, status, devices, session, config, doctor.
- FR-14 Privacy: local-first, consent cloud, credential encryption, remote auth.

## 6. Overlay requirements
Overlay adalah window desktop kecil yang selalu berada di atas aplikasi lain, terutama untuk Live Interview pada satu PC.

Minimum capability:
- Compact transcript dari remote dan local user.
- AI response terbaru.
- Status capture.
- Quick actions: Explain, Translate, Suggest, Hide.
- Pin/unpin dan drag posisi.
- Opacity dan size adjustment.
- Click-through optional untuk area non-interaktif.
- Tidak boleh menutupi seluruh layar secara default.

Technical direction:
- Tauri window dengan `always_on_top`.
- Transparent atau translucent background.
- `ignore_cursor_events` hanya untuk area yang memang tidak interaktif.
- Overlay harus bisa disembunyikan sepenuhnya.
- Overlay tidak boleh menjadi satu-satunya UI; workspace penuh tetap tersedia.

## 7. Non-goals MVP
- Bypass proctoring atau penyamaran proses.
- Identifikasi nama setiap peserta meeting.
- Sinkronisasi akun lintas device.
- Penyimpanan audio mentah secara default.
- Auto-answer sebagai default behavior.

## 8. MVP definition of done
- User dapat membuat session di tiga kategori.
- Setiap session memiliki timeline dan history.
- Mock dan learning menerima mic serta teks.
- Live interview menampilkan transcript mic dan system audio.
- STT, thinker, TTS dikonfigurasi terpisah.
- Remote web dapat membuka session dan mengirim aksi AI.
- Desktop Tauri menjalankan web UI dan mengelola core agent.
- Overlay dapat dibuka, dipindah, disembunyikan, dan menampilkan transcript/AI response.
- CLI dapat menjalankan dan menghentikan agent.
