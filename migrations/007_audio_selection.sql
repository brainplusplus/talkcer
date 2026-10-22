CREATE TABLE audio_selection (
  id TEXT PRIMARY KEY CHECK (id = 'audio'),
  microphone TEXT NOT NULL DEFAULT '',
  playback TEXT NOT NULL DEFAULT '',
  microphone_model TEXT NOT NULL DEFAULT '',
  system_model TEXT NOT NULL DEFAULT '',
  updated_at TEXT NOT NULL
);
