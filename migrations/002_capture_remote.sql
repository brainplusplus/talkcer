CREATE TABLE capture_sessions (
  session_id TEXT PRIMARY KEY REFERENCES sessions(id) ON DELETE CASCADE,
  platform TEXT NOT NULL CHECK (platform = 'windows'),
  state TEXT NOT NULL CHECK (state IN ('idle','capturing','paused','error')),
  error TEXT,
  updated_at TEXT NOT NULL
);

CREATE TABLE remote_access (
  id TEXT PRIMARY KEY,
  enabled INTEGER NOT NULL,
  token_hash TEXT,
  created_at TEXT NOT NULL,
  updated_at TEXT NOT NULL
);
