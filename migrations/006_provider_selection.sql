CREATE TABLE provider_selections (
  capability TEXT PRIMARY KEY CHECK (capability IN ('stt','thinker','tts')),
  offered TEXT NOT NULL DEFAULT '[]',
  disabled TEXT NOT NULL DEFAULT '[]',
  defaults TEXT NOT NULL DEFAULT '{}',
  updated_at TEXT NOT NULL
);
