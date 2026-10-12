CREATE TABLE profile (
  id TEXT PRIMARY KEY,
  name TEXT NOT NULL DEFAULT '',
  birth_date TEXT NOT NULL DEFAULT '',
  birth_place TEXT NOT NULL DEFAULT '',
  phone TEXT NOT NULL DEFAULT '',
  email TEXT NOT NULL DEFAULT '',
  github TEXT NOT NULL DEFAULT '',
  linkedin TEXT NOT NULL DEFAULT '',
  career TEXT NOT NULL DEFAULT '[]',
  education TEXT NOT NULL DEFAULT '[]',
  skills TEXT NOT NULL DEFAULT '[]',
  updated_at TEXT NOT NULL
);
