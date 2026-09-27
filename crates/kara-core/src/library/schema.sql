CREATE TABLE provider_account (
  provider     TEXT PRIMARY KEY CHECK (provider IN ('spotify','apple')),
  display_name TEXT,
  connected_at INTEGER
);

CREATE TABLE track (
  id            INTEGER PRIMARY KEY,
  provider      TEXT NOT NULL CHECK (provider IN ('local','spotify','apple')),
  provider_ref  TEXT,
  title         TEXT NOT NULL,
  artist        TEXT,
  album         TEXT,
  duration_ms   INTEGER,
  artwork_path  TEXT,
  added_at      INTEGER NOT NULL,
  vocal_removal INTEGER NOT NULL DEFAULT 100 CHECK (vocal_removal BETWEEN 0 AND 100),
  key_semitones INTEGER NOT NULL DEFAULT 0 CHECK (key_semitones BETWEEN -6 AND 6),
  UNIQUE (provider, provider_ref)
);

CREATE TABLE collection (
  id           INTEGER PRIMARY KEY,
  provider     TEXT NOT NULL CHECK (provider IN ('local','spotify','apple')),
  kind         TEXT NOT NULL CHECK (kind IN ('playlist','album','artist')),
  provider_ref TEXT NOT NULL,
  name         TEXT NOT NULL,
  subtitle     TEXT,
  artwork_path TEXT,
  UNIQUE (provider, kind, provider_ref)
);

CREATE TABLE collection_track (
  collection_id INTEGER NOT NULL REFERENCES collection ON DELETE CASCADE,
  track_id      INTEGER NOT NULL REFERENCES track ON DELETE CASCADE,
  position      INTEGER NOT NULL,
  PRIMARY KEY (collection_id, track_id)
);

CREATE TABLE audio_source (
  id              INTEGER PRIMARY KEY,
  track_id        INTEGER NOT NULL REFERENCES track ON DELETE CASCADE,
  kind            TEXT NOT NULL CHECK (kind IN ('file','link','match')),
  uri             TEXT NOT NULL,
  label           TEXT,
  duration_ms     INTEGER,
  selected        INTEGER NOT NULL DEFAULT 0,
  audio_hash      TEXT,
  lyric_offset_ms INTEGER NOT NULL DEFAULT 0,
  status          TEXT NOT NULL CHECK (status IN ('pending','fetching','ready','failed')),
  error           TEXT
);
CREATE UNIQUE INDEX one_selected_source ON audio_source(track_id) WHERE selected = 1;
CREATE INDEX audio_source_hash ON audio_source(audio_hash);

CREATE TABLE separation (
  audio_hash   TEXT NOT NULL,
  model_id     TEXT NOT NULL,
  chunk_ms     INTEGER NOT NULL,
  chunks_total INTEGER NOT NULL,
  chunks_done  INTEGER NOT NULL DEFAULT 0,
  status       TEXT NOT NULL CHECK (status IN ('queued','running','ready','failed','cancelled')),
  last_used_at INTEGER,
  PRIMARY KEY (audio_hash, model_id)
);

CREATE TABLE lyrics (
  track_id   INTEGER PRIMARY KEY REFERENCES track ON DELETE CASCADE,
  source     TEXT NOT NULL CHECK (source IN ('lrclib','embedded','none')),
  lines      TEXT,
  fetched_at INTEGER NOT NULL
);

CREATE TABLE setting (key TEXT PRIMARY KEY, value TEXT);

CREATE VIRTUAL TABLE track_fts USING fts5(
  title, artist, album, content='track', content_rowid='id',
  tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TRIGGER track_ai AFTER INSERT ON track BEGIN
  INSERT INTO track_fts(rowid, title, artist, album) VALUES (new.id, new.title, new.artist, new.album);
END;
CREATE TRIGGER track_ad AFTER DELETE ON track BEGIN
  INSERT INTO track_fts(track_fts, rowid, title, artist, album) VALUES ('delete', old.id, old.title, old.artist, old.album);
END;
CREATE TRIGGER track_au AFTER UPDATE OF title, artist, album ON track BEGIN
  INSERT INTO track_fts(track_fts, rowid, title, artist, album) VALUES ('delete', old.id, old.title, old.artist, old.album);
  INSERT INTO track_fts(rowid, title, artist, album) VALUES (new.id, new.title, new.artist, new.album);
END;
