# Talkcer — UI Specification

## Routes
- `/` redirect ke `/live`
- `/live` Live Interview list
- `/live/new` New live interview
- `/live/:id` Live session
- `/mock` Mock Interview list
- `/mock/new` New mock interview
- `/mock/:id` Mock session
- `/learning` Learning list
- `/learning/new` New learning
- `/learning/:id` Learning session
- `/settings/ai` AI configuration
- `/settings/general` General settings
- `/settings/desktop` Desktop & overlay settings

## Layout
- Left sidebar: kategori dan history.
- Main area: session timeline.
- Bottom composer: mic, system audio toggle, text input, action buttons.
- Right panel optional: context, attachments, configuration.

## Components
- `SessionSidebar`
- `SessionList`
- `NewSessionDialog`
- `Timeline`
- `TimelineItem`
- `TranscriptBubble`
- `ChatComposer`
- `MicButton`
- `SystemAudioToggle`
- `AssistantActionBar`
- `AIConfigForm`
- `DevicePicker`
- `ConnectionStatus`
- `OverlayWindow`
- `OverlayTranscript`
- `OverlayQuickActions`

## Overlay UI
```text
┌──────────────────────────────────────────────┐
│ Talkcer Overlay            ● Live   [–][×]   │
├──────────────────────────────────────────────┤
│ Interviewer                                  │
│ How would you handle retries?                │
│                                              │
│ Saya                                         │
│ I would use idempotency keys...              │
│                                              │
│ AI                                           │
│ Jelaskan idempotency dan deduplication.      │
├──────────────────────────────────────────────┤
│ [Explain] [Translate] [Suggest] [Hide]       │
└──────────────────────────────────────────────┘
```

## Overlay behavior
- Default size kecil dan compact.
- Bisa dipindah.
- Bisa di-pin agar selalu on top.
- Bisa di-hide lewat tombol, tray, atau shortcut.
- Bisa diatur opacity.
- Area non-interaktif dapat click-through.
- Overlay menampilkan hanya session aktif.
- Overlay tidak boleh memaksa pengguna meninggalkan aplikasi interview.

## States
Setiap screen harus memiliki:
- Empty state.
- Loading state.
- Error state.
- Partial streaming state.
- Final state.
- Disabled state saat capture tidak aktif.
