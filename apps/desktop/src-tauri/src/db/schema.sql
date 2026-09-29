CREATE TABLE IF NOT EXISTS history (
  id            TEXT PRIMARY KEY,
  kind          TEXT NOT NULL CHECK (kind IN ('tts','stt')),
  created_at    INTEGER NOT NULL,
  title         TEXT NOT NULL,
  text          TEXT NOT NULL,
  audio_path    TEXT,
  duration_ms   INTEGER,
  model_id      TEXT NOT NULL,
  voice_id      TEXT,
  language      TEXT,
  device        TEXT NOT NULL,
  processing_ms INTEGER NOT NULL,
  favorite      INTEGER NOT NULL DEFAULT 0,
  segments_json TEXT
);

CREATE INDEX IF NOT EXISTS idx_history_created ON history(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_history_kind ON history(kind);
CREATE INDEX IF NOT EXISTS idx_history_favorite ON history(favorite);

CREATE VIRTUAL TABLE IF NOT EXISTS history_fts USING fts5(
  title, text, content='history', content_rowid='rowid'
);

CREATE TRIGGER IF NOT EXISTS history_ai AFTER INSERT ON history BEGIN
  INSERT INTO history_fts(rowid, title, text) VALUES (new.rowid, new.title, new.text);
END;

CREATE TRIGGER IF NOT EXISTS history_ad AFTER DELETE ON history BEGIN
  INSERT INTO history_fts(history_fts, rowid, title, text) VALUES ('delete', old.rowid, old.title, old.text);
END;

CREATE TRIGGER IF NOT EXISTS history_au AFTER UPDATE ON history BEGIN
  INSERT INTO history_fts(history_fts, rowid, title, text) VALUES ('delete', old.rowid, old.title, old.text);
  INSERT INTO history_fts(rowid, title, text) VALUES (new.rowid, new.title, new.text);
END;