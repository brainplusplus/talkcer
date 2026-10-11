# Talkcer — Database Schema

```sql
CREATE TABLE sessions (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  mode TEXT NOT NULL CHECK (mode IN ('live_interview','mock_interview','learning')),
  status TEXT NOT NULL CHECK (status IN ('ready','active','paused','completed','archived')),
  context TEXT NOT NULL DEFAULT '{}',
  configuration TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE INDEX idx_sessions_mode ON sessions(mode);
CREATE INDEX idx_sessions_updated_at ON sessions(updated_at DESC);

CREATE TABLE timeline_items (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  kind TEXT NOT NULL,
  speaker TEXT NOT NULL,
  source TEXT NOT NULL,
  purpose TEXT NOT NULL,
  text TEXT NOT NULL,
  started_at_ms INTEGER NOT NULL,
  ended_at_ms INTEGER,
  is_final INTEGER NOT NULL DEFAULT 0,
  metadata TEXT NOT NULL DEFAULT '{}',
  created_at TEXT NOT NULL
);

CREATE INDEX idx_timeline_session_time ON timeline_items(session_id, started_at_ms);
CREATE INDEX idx_timeline_session_final ON timeline_items(session_id, is_final);

CREATE TABLE ai_configurations (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL,
  stt_config TEXT NOT NULL,
  thinker_config TEXT NOT NULL,
  tts_config TEXT NOT NULL,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);

CREATE TABLE attachments (
  id TEXT PRIMARY KEY,
  session_id TEXT NOT NULL REFERENCES sessions(id) ON DELETE CASCADE,
  filename TEXT NOT NULL,
  mime_type TEXT NOT NULL,
  storage_path TEXT NOT NULL,
  created_at TEXT NOT NULL
);

CREATE TABLE overlay_preferences (
  id TEXT PRIMARY KEY,
  enabled INTEGER NOT NULL DEFAULT 0,
  always_on_top INTEGER NOT NULL DEFAULT 1,
  opacity REAL NOT NULL DEFAULT 0.95,
  width INTEGER NOT NULL DEFAULT 420,
  height INTEGER NOT NULL DEFAULT 320,
  position_x INTEGER NOT NULL DEFAULT 0,
  position_y INTEGER NOT NULL DEFAULT 0,
  click_through_non_interactive_area INTEGER NOT NULL DEFAULT 0,
  show_remote_transcript INTEGER NOT NULL DEFAULT 1,
  show_local_transcript INTEGER NOT NULL DEFAULT 1,
  show_ai_response INTEGER NOT NULL DEFAULT 1,
  updated_at TEXT NOT NULL
);
```

## Notes
- Gunakan UUIDv7 atau ULID untuk ID agar sortable.
- Simpan JSON sebagai TEXT di SQLite.
- Tambahkan full-text search pada fase berikutnya.
