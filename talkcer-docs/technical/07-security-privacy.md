# Talkcer — Security & Privacy

## Defaults
- Core API bind `127.0.0.1:54000`.
- Remote access disabled by default.
- Transcripts stored locally.
- Raw audio storage disabled by default.
- Cloud provider calls require explicit configuration.

## Remote access
- Pairing code atau device token.
- HTTPS/WSS saat diakses selain localhost.
- Session-scoped authorization.
- Rate limiting.
- Audit log untuk remote command.

## Credentials
- API key tidak boleh disimpan plaintext.
- Gunakan OS keychain bila tersedia, atau encrypted local store.
- Jangan mengirim credential ke client UI.

## Cloud consent
- Tampilkan indikator cloud usage.
- Jangan fallback otomatis local -> cloud tanpa izin.
- Simpan provider yang dipakai pada setiap session/run.

## Overlay privacy
- Overlay hanya menampilkan session yang dipilih pengguna.
- Overlay tidak boleh menampilkan transcript lain secara default.
- Hide overlay harus benar-benar menyembunyikan window.
- Jangan merekam layar atau mengirim screenshot tanpa izin eksplisit.
