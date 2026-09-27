//! The library database: tracks, collections, audio sources, separation
//! progress, lyrics and settings. One SQLite file (WAL), search via FTS5.

use crate::lyrics::Line;
use crate::problem::Problem;
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Row, Transaction};
use std::path::Path;

/// A string-backed enum with `as_str`, `FromStr`, and SQLite conversions.
macro_rules! str_enum {
    ($name:ident { $($var:ident => $s:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $name { $($var),+ }
        impl $name {
            pub fn as_str(self) -> &'static str { match self { $(Self::$var => $s),+ } }
        }
        impl std::str::FromStr for $name {
            type Err = anyhow::Error;
            fn from_str(s: &str) -> Result<Self> {
                match s { $($s => Ok(Self::$var),)+ _ => anyhow::bail!("unknown {}: {}", stringify!($name), s) }
            }
        }
        impl rusqlite::types::ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> { Ok(self.as_str().into()) }
        }
        impl rusqlite::types::FromSql for $name {
            fn column_result(v: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
                v.as_str()?.parse().map_err(|e: anyhow::Error| rusqlite::types::FromSqlError::Other(e.into()))
            }
        }
        impl serde::Serialize for $name {
            fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
                s.serialize_str(self.as_str())
            }
        }
    };
}

str_enum!(ProviderId { Local => "local", Spotify => "spotify", Apple => "apple" });
str_enum!(CollectionKind { Playlist => "playlist", Album => "album", Artist => "artist" });
str_enum!(SourceKind { File => "file", Link => "link", Match => "match" });
str_enum!(SourceStatus { Pending => "pending", Fetching => "fetching", Ready => "ready", Failed => "failed" });
str_enum!(SepStatus { Queued => "queued", Running => "running", Ready => "ready", Failed => "failed", Cancelled => "cancelled" });
str_enum!(LyricsSource { Lrclib => "lrclib", Embedded => "embedded", None => "none" });

pub struct NewTrack<'a> {
    pub provider: ProviderId,
    pub provider_ref: Option<&'a str>,
    pub title: &'a str,
    pub artist: Option<&'a str>,
    pub album: Option<&'a str>,
    pub duration_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Track {
    pub id: i64,
    pub provider: ProviderId,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_ms: Option<i64>,
    pub vocal_removal: u8,
    pub key_semitones: i8,
    pub instrumental: bool,
    pub artwork_path: Option<String>,
    pub art_seed: u16,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionRow {
    pub id: i64,
    pub provider: ProviderId,
    pub kind: CollectionKind,
    pub name: String,
    pub subtitle: Option<String>,
    /// A user-made playlist, as opposed to Local › Imported or an album/artist.
    pub user: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioSource {
    pub id: i64,
    pub track_id: i64,
    pub kind: SourceKind,
    pub uri: String,
    pub label: Option<String>,
    pub audio_hash: Option<String>,
    pub lyric_offset_ms: i64,
    pub status: SourceStatus,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SeparationRow {
    pub audio_hash: String,
    pub model_id: String,
    pub chunk_ms: u32,
    pub chunks_total: u32,
    pub chunks_done: u32,
    pub status: SepStatus,
    pub last_used_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LyricsRow {
    pub source: LyricsSource,
    pub lines: Vec<Line>,
    pub fetched_at: i64,
}

const TRACK_COLS: &str = "t.id, t.provider, t.title, t.artist, t.album, t.duration_ms, t.vocal_removal, t.key_semitones, t.instrumental, t.artwork_path, t.art_seed";
const SOURCE_COLS: &str = "id, track_id, kind, uri, label, audio_hash, lyric_offset_ms, status, error";
const SEP_COLS: &str = "audio_hash, model_id, chunk_ms, chunks_total, chunks_done, status, last_used_at";
const COLLECTION_COLS: &str = "id, provider, kind, name, subtitle, provider_ref LIKE 'user:%'";

fn track_row(r: &Row) -> rusqlite::Result<Track> {
    Ok(Track {
        id: r.get(0)?,
        provider: r.get(1)?,
        title: r.get(2)?,
        artist: r.get(3)?,
        album: r.get(4)?,
        duration_ms: r.get(5)?,
        vocal_removal: r.get(6)?,
        key_semitones: r.get(7)?,
        instrumental: r.get(8)?,
        artwork_path: r.get(9)?,
        art_seed: r.get(10)?,
    })
}

fn source_row(r: &Row) -> rusqlite::Result<AudioSource> {
    Ok(AudioSource {
        id: r.get(0)?,
        track_id: r.get(1)?,
        kind: r.get(2)?,
        uri: r.get(3)?,
        label: r.get(4)?,
        audio_hash: r.get(5)?,
        lyric_offset_ms: r.get(6)?,
        status: r.get(7)?,
        error: r.get(8)?,
    })
}

fn collection_row(r: &Row) -> rusqlite::Result<CollectionRow> {
    Ok(CollectionRow { id: r.get(0)?, provider: r.get(1)?, kind: r.get(2)?, name: r.get(3)?, subtitle: r.get(4)?, user: r.get(5)? })
}

fn sep_row(r: &Row) -> rusqlite::Result<SeparationRow> {
    Ok(SeparationRow {
        audio_hash: r.get(0)?,
        model_id: r.get(1)?,
        chunk_ms: r.get(2)?,
        chunks_total: r.get(3)?,
        chunks_done: r.get(4)?,
        status: r.get(5)?,
        last_used_at: r.get(6)?,
    })
}

/// Turns on write-ahead logging, retrying while another connection is switching it.
fn enable_wal(conn: &Connection) -> Result<()> {
    let switch = || conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()));
    for _ in 0..100 {
        match switch() {
            Err(rusqlite::Error::SqliteFailure(e, _)) if e.code == rusqlite::ErrorCode::DatabaseBusy => {
                std::thread::sleep(std::time::Duration::from_millis(10))
            }
            r => return Ok(r?),
        }
    }
    Ok(switch()?)
}

/// Whether every foreign key in the database points at a row that exists.
fn no_foreign_key_violations(conn: &Connection) -> Result<bool> {
    let mut stmt = conn.prepare("PRAGMA foreign_key_check")?;
    let ok = stmt.query([])?.next()?.is_none();
    Ok(ok)
}

/// Upgrades the library's tables (and files, found beside `tx.path()`) by one version.
type Step = fn(&Transaction) -> Result<()>;

/// The upgrade steps in order; after step `i` the library is at version `i + 1`.
const STEPS: &[Step] = &[
    |tx| Ok(tx.execute_batch(include_str!("schema.sql"))?),
    |tx| Ok(tx.execute_batch("ALTER TABLE track ADD COLUMN instrumental INTEGER NOT NULL DEFAULT 0")?),
    |tx| Ok(tx.execute_batch("ALTER TABLE track ADD COLUMN art_seed INTEGER NOT NULL DEFAULT 0; UPDATE track SET art_seed = abs(random()) % 360;")?),
];

pub struct Library {
    conn: Connection,
}

impl Library {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path).with_context(|| format!("open {}", path.display()))?, STEPS)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?, STEPS)
    }

    /// Opens the database and runs, in one transaction with foreign keys off, the `steps` it hasn't had; fails if they leave a broken reference.
    fn init(mut conn: Connection, steps: &[Step]) -> Result<Self> {
        enable_wal(&conn)?;
        conn.pragma_update(None, "foreign_keys", "OFF")?;
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let version: usize = tx.pragma_query_value(None, "user_version", |r| r.get(0))?;
        anyhow::ensure!(version <= steps.len(), Problem::NewerLibrary);
        for step in &steps[version..] {
            step(&tx)?;
        }
        tx.pragma_update(None, "user_version", steps.len())?;
        anyhow::ensure!(no_foreign_key_violations(&tx)?, Problem::UpgradeFailed);
        tx.commit()?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        Ok(Self { conn })
    }

    // ---- tracks ----

    pub fn add_track(&self, t: &NewTrack) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO track (provider, provider_ref, title, artist, album, duration_ms, added_at, art_seed) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, abs(random()) % 360)",
            params![t.provider, t.provider_ref, t.title, t.artist, t.album, t.duration_ms, crate::now_ms()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn track(&self, id: i64) -> Result<Track> {
        let sql = format!("SELECT {TRACK_COLS} FROM track t WHERE t.id = ?1");
        self.conn.query_row(&sql, [id], track_row).with_context(|| format!("track {id}"))
    }

    pub fn update_track_meta(&self, id: i64, title: &str, artist: Option<&str>, album: Option<&str>) -> Result<()> {
        self.conn.execute("UPDATE track SET title = ?2, artist = ?3, album = ?4 WHERE id = ?1", params![id, title, artist, album])?;
        Ok(())
    }

    pub fn set_track_settings(&self, id: i64, vocal_removal: u8, key_semitones: i8) -> Result<()> {
        self.conn.execute("UPDATE track SET vocal_removal = ?2, key_semitones = ?3 WHERE id = ?1", params![id, vocal_removal, key_semitones])?;
        Ok(())
    }

    /// Flags a song as already instrumental; the first time, its singer slider moves to original.
    pub fn mark_instrumental(&self, id: i64) -> Result<()> {
        self.conn.execute("UPDATE track SET instrumental = 1, vocal_removal = 0 WHERE id = ?1 AND instrumental = 0", [id])?;
        Ok(())
    }

    /// Sets a song's artwork if it doesn't have one yet; the first picture a song gets is kept.
    pub fn set_artwork(&self, id: i64, path: &str) -> Result<()> {
        self.conn.execute("UPDATE track SET artwork_path = ?2 WHERE id = ?1 AND artwork_path IS NULL", params![id, path])?;
        Ok(())
    }

    /// Deletes a song with its sources, lyrics and collection entries.
    pub fn delete_track(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM track WHERE id = ?1", [id])?;
        Ok(())
    }

    /// Prefix search over title/artist/album; accent-insensitive; never a syntax error.
    pub fn search(&self, query: &str, limit: u32) -> Result<Vec<Track>> {
        let terms: Vec<String> = query
            .split_whitespace()
            .map(|w| format!("\"{}\"*", w.chars().filter(|c| !c.is_control()).collect::<String>().replace('"', "\"\"")))
            .collect();
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT {TRACK_COLS} FROM track_fts JOIN track t ON t.id = track_fts.rowid WHERE track_fts MATCH ?1 ORDER BY rank LIMIT ?2"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![terms.join(" "), limit], track_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // ---- collections ----

    pub fn upsert_collection(&self, provider: ProviderId, kind: CollectionKind, provider_ref: &str, name: &str, subtitle: Option<&str>) -> Result<i64> {
        Ok(self.conn.query_row(
            "INSERT INTO collection (provider, kind, provider_ref, name, subtitle) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (provider, kind, provider_ref) DO UPDATE SET name = excluded.name, subtitle = excluded.subtitle
             RETURNING id",
            params![provider, kind, provider_ref, name, subtitle],
            |r| r.get(0),
        )?)
    }

    /// `provider = None` is the All view.
    pub fn collections(&self, provider: Option<ProviderId>, kind: CollectionKind) -> Result<Vec<CollectionRow>> {
        let sql = format!("SELECT {COLLECTION_COLS} FROM collection WHERE kind = ?1 AND (?2 IS NULL OR provider = ?2) ORDER BY name COLLATE NOCASE");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![kind, provider], collection_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn collection(&self, id: i64) -> Result<CollectionRow> {
        let sql = format!("SELECT {COLLECTION_COLS} FROM collection WHERE id = ?1");
        self.conn.query_row(&sql, [id], collection_row).with_context(|| format!("collection {id}"))
    }

    /// A new, empty user playlist under Local.
    pub fn create_playlist(&self, name: &str) -> Result<i64> {
        Ok(self.conn.query_row(
            "INSERT INTO collection (provider, kind, provider_ref, name)
             VALUES ('local', 'playlist', 'user:' || (SELECT COALESCE(MAX(id), 0) + 1 FROM collection), ?1) RETURNING id",
            [name],
            |r| r.get(0),
        )?)
    }

    /// Renames a user playlist; other collections are left alone.
    pub fn rename_playlist(&self, id: i64, name: &str) -> Result<()> {
        self.conn.execute("UPDATE collection SET name = ?2 WHERE id = ?1 AND provider_ref LIKE 'user:%'", params![id, name])?;
        Ok(())
    }

    /// Deletes a user playlist; its songs stay in the library.
    pub fn delete_playlist(&self, id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM collection WHERE id = ?1 AND provider_ref LIKE 'user:%'", [id])?;
        Ok(())
    }

    /// Appends at the end; false when the song was already there.
    pub fn add_to_collection(&self, collection_id: i64, track_id: i64) -> Result<bool> {
        Ok(self.conn.execute(
            "INSERT OR IGNORE INTO collection_track (collection_id, track_id, position)
             VALUES (?1, ?2, (SELECT COALESCE(MAX(position) + 1, 0) FROM collection_track WHERE collection_id = ?1))",
            params![collection_id, track_id],
        )? > 0)
    }

    /// Puts a song at the top; false when it was already there.
    pub fn add_to_front(&self, collection_id: i64, track_id: i64) -> Result<bool> {
        Ok(self.conn.execute(
            "INSERT OR IGNORE INTO collection_track (collection_id, track_id, position)
             VALUES (?1, ?2, (SELECT COALESCE(MIN(position) - 1, 0) FROM collection_track WHERE collection_id = ?1))",
            params![collection_id, track_id],
        )? > 0)
    }

    /// Ids of every collection holding this song.
    pub fn collections_of(&self, track_id: i64) -> Result<Vec<i64>> {
        let mut stmt = self.conn.prepare("SELECT collection_id FROM collection_track WHERE track_id = ?1")?;
        let rows = stmt.query_map([track_id], |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn remove_from_collection(&self, collection_id: i64, track_id: i64) -> Result<()> {
        self.conn.execute("DELETE FROM collection_track WHERE collection_id = ?1 AND track_id = ?2", [collection_id, track_id])?;
        Ok(())
    }

    /// Moves a song to index `to` of the collection's order.
    pub fn move_in_collection(&self, collection_id: i64, track_id: i64, to: usize) -> Result<()> {
        let mut ids: Vec<i64> = self.collection_tracks(collection_id)?.into_iter().map(|t| t.id).collect();
        let Some(from) = ids.iter().position(|&t| t == track_id) else { return Ok(()) };
        ids.remove(from);
        ids.insert(to.min(ids.len()), track_id);
        let tx = self.conn.unchecked_transaction()?;
        for (pos, t) in ids.iter().enumerate() {
            tx.execute("UPDATE collection_track SET position = ?3 WHERE collection_id = ?1 AND track_id = ?2", params![collection_id, t, pos as i64])?;
        }
        tx.commit()?;
        Ok(())
    }

    /// Takes a song out of every collection of `kind`.
    pub fn leave_collections(&self, track_id: i64, kind: CollectionKind) -> Result<()> {
        self.conn.execute(
            "DELETE FROM collection_track WHERE track_id = ?1 AND collection_id IN (SELECT id FROM collection WHERE kind = ?2)",
            params![track_id, kind],
        )?;
        Ok(())
    }

    /// Deletes albums and artists that no longer hold any song.
    pub fn prune_empty_collections(&self) -> Result<()> {
        self.conn.execute(
            "DELETE FROM collection WHERE kind IN ('album', 'artist') AND id NOT IN (SELECT collection_id FROM collection_track)",
            [],
        )?;
        Ok(())
    }

    pub fn collection_tracks(&self, collection_id: i64) -> Result<Vec<Track>> {
        let sql = format!(
            "SELECT {TRACK_COLS} FROM collection_track ct JOIN track t ON t.id = ct.track_id WHERE ct.collection_id = ?1 ORDER BY ct.position"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([collection_id], track_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // ---- audio sources ----

    /// The first source added to a track becomes its selected source.
    pub fn add_source(&self, track_id: i64, kind: SourceKind, uri: &str, label: Option<&str>) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO audio_source (track_id, kind, uri, label, status, selected)
             VALUES (?1, ?2, ?3, ?4, 'pending', NOT EXISTS (SELECT 1 FROM audio_source WHERE track_id = ?1))",
            params![track_id, kind, uri, label],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn selected_source(&self, track_id: i64) -> Result<Option<AudioSource>> {
        let sql = format!("SELECT {SOURCE_COLS} FROM audio_source WHERE track_id = ?1 AND selected = 1");
        Ok(self.conn.query_row(&sql, [track_id], source_row).optional()?)
    }

    pub fn source_by_uri(&self, kind: SourceKind, uri: &str) -> Result<Option<AudioSource>> {
        let sql = format!("SELECT {SOURCE_COLS} FROM audio_source WHERE kind = ?1 AND uri = ?2");
        Ok(self.conn.query_row(&sql, params![kind, uri], source_row).optional()?)
    }

    pub fn sources_with_hash(&self, hash: &str) -> Result<Vec<AudioSource>> {
        let sql = format!("SELECT {SOURCE_COLS} FROM audio_source WHERE audio_hash = ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([hash], source_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn set_source_status(&self, id: i64, status: SourceStatus, error: Option<&str>) -> Result<()> {
        self.conn.execute("UPDATE audio_source SET status = ?2, error = ?3 WHERE id = ?1", params![id, status, error])?;
        Ok(())
    }

    /// Records the standardized audio's hash; the source becomes ready.
    pub fn set_source_audio(&self, id: i64, hash: &str, duration_ms: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE audio_source SET audio_hash = ?2, duration_ms = ?3, status = 'ready', error = NULL WHERE id = ?1",
            params![id, hash, duration_ms],
        )?;
        self.conn.execute(
            "UPDATE track SET duration_ms = COALESCE(duration_ms, ?2) WHERE id = (SELECT track_id FROM audio_source WHERE id = ?1)",
            params![id, duration_ms],
        )?;
        Ok(())
    }

    /// After eviction: these sources must be fetched again before playing.
    pub fn reset_sources_for_hash(&self, hash: &str) -> Result<()> {
        self.conn.execute("UPDATE audio_source SET status = 'pending' WHERE audio_hash = ?1", [hash])?;
        Ok(())
    }

    pub fn set_lyric_offset(&self, source_id: i64, ms: i64) -> Result<()> {
        self.conn.execute("UPDATE audio_source SET lyric_offset_ms = ?2 WHERE id = ?1", params![source_id, ms])?;
        Ok(())
    }

    // ---- separation ----

    pub fn separation(&self, hash: &str, model_id: &str) -> Result<Option<SeparationRow>> {
        let sql = format!("SELECT {SEP_COLS} FROM separation WHERE audio_hash = ?1 AND model_id = ?2");
        Ok(self.conn.query_row(&sql, [hash, model_id], sep_row).optional()?)
    }

    pub fn separations(&self) -> Result<Vec<SeparationRow>> {
        let sql = format!("SELECT {SEP_COLS} FROM separation");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], sep_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn upsert_separation(&self, r: &SeparationRow) -> Result<()> {
        self.conn.execute(
            &format!("INSERT OR REPLACE INTO separation ({SEP_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)"),
            params![r.audio_hash, r.model_id, r.chunk_ms, r.chunks_total, r.chunks_done, r.status, r.last_used_at],
        )?;
        Ok(())
    }

    pub fn touch_separation(&self, hash: &str, model_id: &str, at_ms: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE separation SET last_used_at = ?3 WHERE audio_hash = ?1 AND model_id = ?2",
            params![hash, model_id, at_ms],
        )?;
        Ok(())
    }

    pub fn delete_separation(&self, hash: &str, model_id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM separation WHERE audio_hash = ?1 AND model_id = ?2", [hash, model_id])?;
        Ok(())
    }

    // ---- lyrics ----

    pub fn lyrics(&self, track_id: i64) -> Result<Option<LyricsRow>> {
        let row = self
            .conn
            .query_row("SELECT source, lines, fetched_at FROM lyrics WHERE track_id = ?1", [track_id], |r| {
                Ok((r.get::<_, LyricsSource>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, i64>(2)?))
            })
            .optional()?;
        row.map(|(source, lines, fetched_at)| {
            let lines = match lines {
                Some(json) => serde_json::from_str(&json)?,
                None => Vec::new(),
            };
            Ok(LyricsRow { source, lines, fetched_at })
        })
        .transpose()
    }

    pub fn set_lyrics(&self, track_id: i64, source: LyricsSource, lines: &[Line], at_ms: i64) -> Result<()> {
        let json = if lines.is_empty() { None } else { Some(serde_json::to_string(lines)?) };
        self.conn.execute(
            "INSERT OR REPLACE INTO lyrics (track_id, source, lines, fetched_at) VALUES (?1, ?2, ?3, ?4)",
            params![track_id, source, json, at_ms],
        )?;
        Ok(())
    }

    // ---- settings ----

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT value FROM setting WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute("INSERT OR REPLACE INTO setting (key, value) VALUES (?1, ?2)", [key, value])?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::{Line, Word};

    fn lib() -> Library {
        Library::open_in_memory().unwrap()
    }

    fn local<'a>(title: &'a str, artist: Option<&'a str>) -> NewTrack<'a> {
        NewTrack { provider: ProviderId::Local, provider_ref: None, title, artist, album: None, duration_ms: Some(1000) }
    }

    #[test]
    fn adds_and_reads_a_track_with_default_settings() {
        let l = lib();
        let id = l.add_track(&local("Paper Satellites", Some("Mina Okada"))).unwrap();
        let t = l.track(id).unwrap();
        assert_eq!((t.title.as_str(), t.artist.as_deref(), t.vocal_removal, t.key_semitones), ("Paper Satellites", Some("Mina Okada"), 100, 0));
        l.set_track_settings(id, 40, -2).unwrap();
        let t = l.track(id).unwrap();
        assert_eq!((t.vocal_removal, t.key_semitones), (40, -2));
    }

    #[test]
    fn rejects_out_of_range_settings() {
        let l = lib();
        let id = l.add_track(&local("x", None)).unwrap();
        assert!(l.set_track_settings(id, 100, 7).is_err());
    }

    #[test]
    fn search_handles_special_characters_and_accents() {
        let l = lib();
        l.add_track(&local("Crazy in Love", Some("Beyoncé"))).unwrap();
        l.add_track(&local("Back in Black", Some("AC/DC"))).unwrap();
        assert_eq!(l.search("beyon", 10).unwrap()[0].title, "Crazy in Love");
        assert_eq!(l.search("back bla", 10).unwrap()[0].artist.as_deref(), Some("AC/DC"));
        assert!(l.search("AC/DC \"Back\" *", 10).is_ok());
        assert!(l.search("   ", 10).unwrap().is_empty());
        assert!(l.search("back\0bla", 10).is_ok());
    }

    #[test]
    fn search_sees_renamed_tracks() {
        let l = lib();
        let id = l.add_track(&local("youtube.com link", None)).unwrap();
        l.update_track_meta(id, "Midnight Laundromat", Some("Kiko"), None).unwrap();
        assert_eq!(l.search("laundro", 10).unwrap().len(), 1);
        assert!(l.search("youtube", 10).unwrap().is_empty());
    }

    #[test]
    fn collections_filter_by_provider_and_keep_order() {
        let l = lib();
        let a = l.add_track(&local("A", None)).unwrap();
        let b = l.add_track(&local("B", None)).unwrap();
        let imported = l.upsert_collection(ProviderId::Local, CollectionKind::Playlist, "imported", "Imported", None).unwrap();
        assert_eq!(imported, l.upsert_collection(ProviderId::Local, CollectionKind::Playlist, "imported", "Imported", None).unwrap());
        l.add_to_collection(imported, b).unwrap();
        l.add_to_collection(imported, a).unwrap();
        l.add_to_collection(imported, b).unwrap(); // duplicate ignored
        let titles: Vec<_> = l.collection_tracks(imported).unwrap().into_iter().map(|t| t.title).collect();
        assert_eq!(titles, vec!["B", "A"]);
        assert_eq!(l.collections(None, CollectionKind::Playlist).unwrap().len(), 1);
        assert_eq!(l.collections(Some(ProviderId::Spotify), CollectionKind::Playlist).unwrap().len(), 0);
        assert_eq!(l.collections(Some(ProviderId::Local), CollectionKind::Album).unwrap().len(), 0);
    }

    #[test]
    fn first_source_is_selected_and_audio_can_be_set() {
        let l = lib();
        let t = l.add_track(&local("A", None)).unwrap();
        let s1 = l.add_source(t, SourceKind::File, "/music/a.mp3", Some("a.mp3")).unwrap();
        l.add_source(t, SourceKind::Link, "https://youtu.be/x", None).unwrap();
        let sel = l.selected_source(t).unwrap().unwrap();
        assert_eq!((sel.id, sel.status), (s1, SourceStatus::Pending));
        l.set_source_audio(s1, "h1", 2000).unwrap();
        let sel = l.selected_source(t).unwrap().unwrap();
        assert_eq!((sel.audio_hash.as_deref(), sel.status), (Some("h1"), SourceStatus::Ready));
        l.set_source_status(s1, SourceStatus::Failed, Some("The file was moved or deleted.")).unwrap();
        assert_eq!(l.selected_source(t).unwrap().unwrap().error.as_deref(), Some("The file was moved or deleted."));
        l.reset_sources_for_hash("h1").unwrap();
        assert_eq!(l.sources_with_hash("h1").unwrap()[0].status, SourceStatus::Pending);
        l.set_lyric_offset(s1, -300).unwrap();
        assert_eq!(l.selected_source(t).unwrap().unwrap().lyric_offset_ms, -300);
    }

    #[test]
    fn separation_rows_roundtrip() {
        let l = lib();
        let mut row = SeparationRow { audio_hash: "h".into(), model_id: "m".into(), chunk_ms: 10_000, chunks_total: 5, chunks_done: 0, status: SepStatus::Running, last_used_at: None };
        l.upsert_separation(&row).unwrap();
        row.chunks_done = 3;
        l.upsert_separation(&row).unwrap();
        l.touch_separation("h", "m", 42).unwrap();
        let got = l.separation("h", "m").unwrap().unwrap();
        assert_eq!((got.chunks_done, got.last_used_at), (3, Some(42)));
        assert_eq!(l.separations().unwrap().len(), 1);
        l.delete_separation("h", "m").unwrap();
        assert!(l.separation("h", "m").unwrap().is_none());
    }

    #[test]
    fn lyrics_and_settings_roundtrip() {
        let l = lib();
        let t = l.add_track(&local("A", None)).unwrap();
        assert!(l.lyrics(t).unwrap().is_none());
        let lines = vec![Line { start_ms: 0, end_ms: 1000, text: "hi there".into(), words: vec![Word { start_ms: 0, end_ms: 500, text: "hi".into() }], voice: None }];
        l.set_lyrics(t, LyricsSource::Lrclib, &lines, 7).unwrap();
        let got = l.lyrics(t).unwrap().unwrap();
        assert_eq!((got.source, got.lines, got.fetched_at), (LyricsSource::Lrclib, lines, 7));
        l.set_lyrics(t, LyricsSource::None, &[], 8).unwrap();
        assert!(l.lyrics(t).unwrap().unwrap().lines.is_empty());
        assert_eq!(l.setting("cache_budget_bytes").unwrap(), None);
        l.set_setting("cache_budget_bytes", "123").unwrap();
        assert_eq!(l.setting("cache_budget_bytes").unwrap().as_deref(), Some("123"));
    }

    #[test]
    fn concurrent_first_opens_of_a_new_database_all_succeed() {
        for _ in 0..20 {
            let dir = tempfile::tempdir().unwrap();
            let p = dir.path().join("kara.db");
            std::thread::scope(|s| {
                let opens: Vec<_> = (0..6).map(|_| s.spawn(|| Library::open(&p).map(|_| ()))).collect();
                for o in opens {
                    o.join().unwrap().unwrap();
                }
            });
        }
    }

    #[test]
    fn opening_runs_the_steps_a_library_has_not_had() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("kara.db");
        let id = Library::open(&p).unwrap().add_track(&local("A", None)).unwrap();
        let steps = [STEPS, &[|tx: &Transaction| Ok(tx.execute_batch("CREATE TABLE added (x INTEGER)")?)]].concat();
        let lib = Library::init(Connection::open(&p).unwrap(), &steps).unwrap();
        assert_eq!(lib.track(id).unwrap().title, "A");
        let version: usize = lib.conn.pragma_query_value(None, "user_version", |r| r.get(0)).unwrap();
        assert_eq!(version, STEPS.len() + 1);
        lib.conn.execute("INSERT INTO added (x) VALUES (1)", []).unwrap();
        let fk: i64 = lib.conn.pragma_query_value(None, "foreign_keys", |r| r.get(0)).unwrap();
        assert_eq!(fk, 1);
    }

    #[test]
    fn an_upgrade_step_that_rebuilds_a_table_keeps_rows_that_reference_it() {
        let setup: Step = |tx| {
            Ok(tx.execute_batch(
                "CREATE TABLE parent (id INTEGER PRIMARY KEY);
                 CREATE TABLE child (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES parent ON DELETE CASCADE);
                 INSERT INTO parent (id) VALUES (1);
                 INSERT INTO child (id, parent_id) VALUES (1, 1);",
            )?)
        };
        let rebuild: Step = |tx| {
            Ok(tx.execute_batch(
                "CREATE TABLE parent_new (id INTEGER PRIMARY KEY);
                 INSERT INTO parent_new SELECT id FROM parent;
                 DROP TABLE parent;
                 ALTER TABLE parent_new RENAME TO parent;",
            )?)
        };
        let lib = Library::init(Connection::open_in_memory().unwrap(), &[setup, rebuild]).unwrap();
        let count: i64 = lib.conn.query_row("SELECT COUNT(*) FROM child", [], |r| r.get(0)).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn a_dangling_reference_left_by_an_upgrade_rolls_back_and_a_corrected_step_still_upgrades() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("kara.db");
        let bad: Step = |tx| {
            Ok(tx.execute_batch(
                "CREATE TABLE parent (id INTEGER PRIMARY KEY);
                 CREATE TABLE child (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES parent);
                 INSERT INTO child (id, parent_id) VALUES (1, 99);",
            )?)
        };
        assert!(Library::init(Connection::open(&p).unwrap(), &[bad]).is_err());

        let good: Step = |tx| {
            Ok(tx.execute_batch(
                "CREATE TABLE parent (id INTEGER PRIMARY KEY);
                 CREATE TABLE child (id INTEGER PRIMARY KEY, parent_id INTEGER REFERENCES parent);
                 INSERT INTO parent (id) VALUES (1);
                 INSERT INTO child (id, parent_id) VALUES (1, 1);",
            )?)
        };
        assert!(Library::init(Connection::open(&p).unwrap(), &[good]).is_ok());
    }

    #[test]
    fn a_library_from_a_newer_kara_is_refused() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("kara.db");
        Connection::open(&p).unwrap().pragma_update(None, "user_version", STEPS.len() + 1).unwrap();
        let err = Library::open(&p).err().unwrap();
        assert_eq!(err.to_string(), "This library was made by a newer version of KaraAlwaysOK. Update KaraAlwaysOK to open it.");
    }

    #[test]
    fn instrumental_songs_start_at_original_and_keep_later_changes() {
        let l = lib();
        let id = l.add_track(&local("Rooftop Static (Instrumental)", None)).unwrap();
        l.mark_instrumental(id).unwrap();
        let t = l.track(id).unwrap();
        assert_eq!((t.instrumental, t.vocal_removal), (true, 0));
        l.set_track_settings(id, 60, 0).unwrap();
        l.mark_instrumental(id).unwrap();
        assert_eq!(l.track(id).unwrap().vocal_removal, 60);
    }

    #[test]
    fn a_version_1_database_gains_the_instrumental_flag() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("kara.db");
        let conn = Connection::open(&p).unwrap();
        conn.execute_batch(include_str!("schema.sql")).unwrap();
        conn.pragma_update(None, "user_version", 1).unwrap();
        conn.execute("INSERT INTO track (provider, title, added_at) VALUES ('local', 'Old Song', 0)", []).unwrap();
        drop(conn);
        let l = Library::open(&p).unwrap();
        assert!(!l.search("old", 1).unwrap()[0].instrumental);
    }

    #[test]
    fn user_playlists_can_be_made_filled_reordered_and_removed() {
        let l = lib();
        let [a, b, c] = ["A", "B", "C"].map(|t| l.add_track(&local(t, None)).unwrap());
        let imported = l.upsert_collection(ProviderId::Local, CollectionKind::Playlist, "imported", "Imported", None).unwrap();
        let mix = l.create_playlist("Friday Mix").unwrap();
        assert_ne!(mix, l.create_playlist("Friday Mix").unwrap());
        for t in [a, b, c] {
            l.add_to_collection(mix, t).unwrap();
        }
        assert!(!l.add_to_collection(mix, a).unwrap());
        assert_eq!(l.collections_of(c).unwrap(), vec![mix]);
        l.move_in_collection(mix, c, 0).unwrap();
        l.remove_from_collection(mix, b).unwrap();
        let titles: Vec<_> = l.collection_tracks(mix).unwrap().into_iter().map(|t| t.title).collect();
        assert_eq!(titles, vec!["C", "A"]);
        l.add_to_front(imported, a).unwrap();
        l.add_to_front(imported, b).unwrap();
        let newest_first: Vec<_> = l.collection_tracks(imported).unwrap().into_iter().map(|t| t.title).collect();
        assert_eq!(newest_first, vec!["B", "A"]);
        l.rename_playlist(mix, "Band Night").unwrap();
        let row = l.collection(mix).unwrap();
        assert_eq!((row.name.as_str(), row.user), ("Band Night", true));
        l.rename_playlist(imported, "Mine").unwrap();
        l.delete_playlist(imported).unwrap();
        assert_eq!(l.collection(imported).unwrap().name, "Imported");
        assert!(!l.collection(imported).unwrap().user);
        l.delete_playlist(mix).unwrap();
        assert!(l.collection(mix).is_err());
        assert_eq!(l.track(a).unwrap().title, "A");
    }

    #[test]
    fn tracks_serialize_for_the_app() {
        let l = lib();
        let id = l.add_track(&local("Paper Satellites", Some("Mina Okada"))).unwrap();
        let json = serde_json::to_value(l.track(id).unwrap()).unwrap();
        assert_eq!(json["provider"], "local");
        assert_eq!(json["vocalRemoval"], 100);
        assert_eq!(json["durationMs"], 1000);
    }

    #[test]
    fn a_song_keeps_the_gradient_it_was_given() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("kara.db");
        let id = Library::open(&p).unwrap().add_track(&local("A", None)).unwrap();
        let seed = Library::open(&p).unwrap().track(id).unwrap().art_seed;
        assert!(seed < 360);
        assert_eq!(Library::open(&p).unwrap().track(id).unwrap().art_seed, seed);
    }

    #[test]
    fn the_first_artwork_a_song_gets_is_kept() {
        let l = lib();
        let id = l.add_track(&local("A", None)).unwrap();
        l.set_artwork(id, "/art/first.png").unwrap();
        l.set_artwork(id, "/art/second.png").unwrap();
        assert_eq!(l.track(id).unwrap().artwork_path.as_deref(), Some("/art/first.png"));
    }
}
