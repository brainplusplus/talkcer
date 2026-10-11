# Platform Matrix

| Feature | Windows | macOS | Linux |
|---|---|---|---|
| Web UI | Yes | Yes | Yes |
| Browser mic | Yes | Yes | Yes |
| Native mic | Planned | Planned | Planned |
| System audio capture | WASAPI loopback | ScreenCaptureKit / process tap | PipeWire/PulseAudio monitor |
| Tauri desktop | Planned | Planned | Planned |
| Overlay | Planned | Planned | Planned |
| Global shortcut | Planned | Planned | Planned |

## Windows audio note
WASAPI loopback dapat menangkap audio dari audio endpoint; Windows juga menyediakan application loopback untuk membatasi capture ke proses tertentu. Perilaku dapat berbeda tergantung driver dan platform, termasuk ARM64. [web:65][web:67][web:68]

MVP target platform harus dipilih sebelum implementasi live capture.
