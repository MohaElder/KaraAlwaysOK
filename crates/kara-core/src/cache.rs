//! The separated-audio cache: chunk files on disk, marking songs ready, the
//! disk budget (least recently played goes first) and cleanup after a crash.

use crate::audio::{encode_flac, read_flac, Stereo};
use crate::library::{Library, SepStatus, SeparationRow, SourceKind};
use crate::separate::mdx::ChunkOut;
use crate::store::{write_atomic, Stem, Store};
use anyhow::{Context, Result};
use std::path::Path;

pub const DEFAULT_BUDGET: u64 = 5 * 1024 * 1024 * 1024;

pub struct Entry {
    pub size_bytes: i64,
    pub last_used_at: i64,
    pub protected: bool,
}

/// Indices to evict, least recently used first, until the total fits `budget`.
pub fn pick_evictions(entries: &[Entry], budget: u64) -> Vec<usize> {
    let mut total: i64 = entries.iter().map(|e| e.size_bytes).sum();
    let mut order: Vec<usize> = (0..entries.len()).filter(|&i| !entries[i].protected).collect();
    order.sort_by_key(|&i| entries[i].last_used_at);
    let mut out = Vec::new();
    for i in order {
        if total <= budget as i64 {
            break;
        }
        total -= entries[i].size_bytes;
        out.push(i);
    }
    out
}

/// Writes the vocals file, then the instrumental file. A chunk counts as done
/// only when both exist.
pub fn write_chunk(store: &Store, hash: &str, model_id: &str, c: &ChunkOut) -> Result<()> {
    let i = c.index as u32;
    write_atomic(&store.chunk_path(hash, model_id, i, Stem::Vocals), &encode_flac(&c.vocals)?)?;
    write_atomic(&store.chunk_path(hash, model_id, i, Stem::Inst), &encode_flac(&c.inst)?)?;
    Ok(())
}

pub fn read_chunk(store: &Store, hash: &str, model_id: &str, index: u32) -> Result<(Stereo, Stereo)> {
    Ok((
        read_flac(&store.chunk_path(hash, model_id, index, Stem::Vocals))?,
        read_flac(&store.chunk_path(hash, model_id, index, Stem::Inst))?,
    ))
}

/// Complete chunk pairs on disk, counting up from 0 and stopping at the first gap.
pub fn count_complete_chunks(store: &Store, hash: &str, model_id: &str) -> u32 {
    let mut n = 0;
    while store.chunk_path(hash, model_id, n, Stem::Vocals).exists() && store.chunk_path(hash, model_id, n, Stem::Inst).exists() {
        n += 1;
    }
    n
}

fn dir_size(dir: &Path) -> i64 {
    std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()?.metadata().ok()).map(|m| m.len() as i64).sum())
        .unwrap_or(0)
}

/// All chunks are on disk: mark ready, record the size, drop the now-redundant source.
pub fn finish(store: &Store, lib: &Library, hash: &str, model_id: &str) -> Result<()> {
    let mut row = lib.separation(hash, model_id)?.context("missing separation row")?;
    row.status = SepStatus::Ready;
    row.chunks_done = row.chunks_total;
    row.size_bytes = dir_size(&store.stems_dir(hash, model_id));
    row.last_used_at = Some(crate::now_ms());
    lib.upsert_separation(&row)?;
    let _ = std::fs::remove_file(store.source_path(hash));
    Ok(())
}

/// The original song, rebuilt from its two separated tracks.
pub fn rebuild_mix(store: &Store, hash: &str, model_id: &str, chunks: u32) -> Result<Stereo> {
    let mut mix = Stereo::default();
    for i in 0..chunks {
        let (v, inst) = read_chunk(store, hash, model_id, i)?;
        mix.left.extend(v.left.iter().zip(&inst.left).map(|(a, b)| a + b));
        mix.right.extend(v.right.iter().zip(&inst.right).map(|(a, b)| a + b));
    }
    Ok(mix)
}

pub fn budget(lib: &Library) -> Result<u64> {
    Ok(lib.setting("cache_budget_bytes")?.and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_BUDGET))
}

/// Deletes the separated audio of least recently played songs until the cache
/// fits its budget. Never touches `protected` hashes (playing / queued), or a
/// song whose only source is a local file that no longer exists.
pub fn enforce_budget(store: &Store, lib: &Library, protected: &[String]) -> Result<Vec<String>> {
    let rows: Vec<SeparationRow> = lib.separations()?.into_iter().filter(|r| r.status == SepStatus::Ready).collect();
    let mut entries = Vec::with_capacity(rows.len());
    for r in &rows {
        let unrecoverable = lib
            .sources_with_hash(&r.audio_hash)?
            .iter()
            .any(|s| s.kind == SourceKind::File && !Path::new(&s.uri).exists());
        entries.push(Entry {
            size_bytes: r.size_bytes,
            last_used_at: r.last_used_at.unwrap_or(0),
            protected: unrecoverable || protected.contains(&r.audio_hash),
        });
    }
    let mut evicted = Vec::new();
    for i in pick_evictions(&entries, budget(lib)?) {
        let r = &rows[i];
        match std::fs::remove_dir_all(store.stems_dir(&r.audio_hash, &r.model_id)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
        let _ = std::fs::remove_dir(store.audio_dir(&r.audio_hash)); // only if now empty
        lib.delete_separation(&r.audio_hash, &r.model_id)?;
        lib.reset_sources_for_hash(&r.audio_hash)?;
        evicted.push(r.audio_hash.clone());
    }
    Ok(evicted)
}

fn remove_part_files(dir: &Path) -> Result<()> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Ok(()) };
    for entry in rd {
        let p = entry?.path();
        if p.is_dir() {
            remove_part_files(&p)?;
        } else if p.extension().is_some_and(|x| x == "part") {
            std::fs::remove_file(&p)?;
        }
    }
    Ok(())
}

/// Run once at launch: clear temp downloads and half-written files, then make
/// the database agree with the chunk files actually on disk.
pub fn startup_cleanup(store: &Store, lib: &Library) -> Result<()> {
    let _ = std::fs::remove_dir_all(store.tmp_dir());
    std::fs::create_dir_all(store.tmp_dir())?;
    remove_part_files(&store.audio_root())?;
    for mut row in lib.separations()? {
        let done = count_complete_chunks(store, &row.audio_hash, &row.model_id);
        if done >= row.chunks_total {
            if row.status != SepStatus::Ready {
                finish(store, lib, &row.audio_hash, &row.model_id)?;
            }
            continue;
        }
        let status = match row.status {
            SepStatus::Ready | SepStatus::Running => SepStatus::Queued,
            s => s,
        };
        if done != row.chunks_done || status != row.status {
            row.chunks_done = done;
            row.status = status;
            lib.upsert_separation(&row)?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{NewTrack, ProviderId};
    use crate::separate::mdx::ChunkOut;

    fn e(size: i64, used: i64, protected: bool) -> Entry {
        Entry { size_bytes: size, last_used_at: used, protected }
    }

    fn chunk(index: usize, len: usize) -> ChunkOut {
        let v: Vec<f32> = (0..len).map(|i| (i as f32 * 0.01).sin() * 0.3).collect();
        let i: Vec<f32> = v.iter().map(|x| x * 0.5).collect();
        ChunkOut { index, vocals: Stereo { left: v.clone(), right: v }, inst: Stereo { left: i.clone(), right: i } }
    }

    fn sep_row(hash: &str, total: u32, status: SepStatus, size: i64, used: i64) -> SeparationRow {
        SeparationRow { audio_hash: hash.into(), model_id: "m".into(), chunk_ms: 10_000, chunks_total: total, chunks_done: total, status, size_bytes: size, last_used_at: Some(used) }
    }

    #[test]
    fn evicts_least_recently_used_until_under_budget() {
        let entries = [e(40, 3, false), e(40, 1, false), e(40, 2, false)];
        assert_eq!(pick_evictions(&entries, 100), vec![1]);
        assert_eq!(pick_evictions(&entries, 40), vec![1, 2]);
        assert!(pick_evictions(&entries, 120).is_empty());
    }

    #[test]
    fn never_evicts_protected_entries() {
        let entries = [e(100, 1, true), e(10, 2, false)];
        assert_eq!(pick_evictions(&entries, 0), vec![1]);
    }

    #[test]
    fn chunks_roundtrip_and_count_only_complete_pairs() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::new(dir.path());
        write_chunk(&s, "h", "m", &chunk(0, 1000)).unwrap();
        write_chunk(&s, "h", "m", &chunk(1, 1000)).unwrap();
        // A lone vocals file for chunk 2 (crash between the two writes) doesn't count.
        std::fs::write(s.chunk_path("h", "m", 2, Stem::Vocals), b"x").unwrap();
        assert_eq!(count_complete_chunks(&s, "h", "m"), 2);
        let (v, i) = read_chunk(&s, "h", "m", 1).unwrap();
        assert_eq!((v.len(), i.len()), (1000, 1000));
    }

    #[test]
    fn rebuilt_mix_is_vocals_plus_instrumental() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::new(dir.path());
        let c = chunk(0, 500);
        write_chunk(&s, "h", "m", &c).unwrap();
        let mix = rebuild_mix(&s, "h", "m", 1).unwrap();
        for k in 0..500 {
            assert!((mix.left[k] - (c.vocals.left[k] + c.inst.left[k])).abs() < 2.0 / 32767.0);
        }
    }

    #[test]
    fn finish_marks_ready_records_size_and_deletes_source() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::new(dir.path());
        let lib = Library::open_in_memory().unwrap();
        write_atomic(&s.source_path("h"), b"src").unwrap();
        write_chunk(&s, "h", "m", &chunk(0, 1000)).unwrap();
        lib.upsert_separation(&sep_row("h", 1, SepStatus::Running, 0, 0)).unwrap();
        finish(&s, &lib, "h", "m").unwrap();
        let row = lib.separation("h", "m").unwrap().unwrap();
        assert_eq!(row.status, SepStatus::Ready);
        assert!(row.size_bytes > 0);
        assert!(!s.source_path("h").exists());
    }

    #[test]
    fn enforce_budget_evicts_and_resets_sources_but_keeps_unrecoverable_files() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::new(dir.path());
        let lib = Library::open_in_memory().unwrap();
        lib.set_setting("cache_budget_bytes", "100").unwrap();
        let t = |name| lib.add_track(&NewTrack { provider: ProviderId::Local, provider_ref: None, title: name, artist: None, album: None, duration_ms: None }).unwrap();
        // "old": a link (can be downloaded again). "gone": a file whose original was deleted.
        let (old, gone) = (t("old"), t("gone"));
        let s_old = lib.add_source(old, SourceKind::Link, "https://youtu.be/x", None).unwrap();
        let s_gone = lib.add_source(gone, SourceKind::File, "/nowhere/gone.wav", None).unwrap();
        lib.set_source_audio(s_old, "old", 1000).unwrap();
        lib.set_source_audio(s_gone, "gone", 1000).unwrap();
        for h in ["old", "gone", "new"] {
            write_chunk(&s, h, "m", &chunk(0, 10)).unwrap();
        }
        lib.upsert_separation(&sep_row("gone", 1, SepStatus::Ready, 80, 1)).unwrap();
        lib.upsert_separation(&sep_row("old", 1, SepStatus::Ready, 80, 2)).unwrap();
        lib.upsert_separation(&sep_row("new", 1, SepStatus::Ready, 80, 3)).unwrap();
        let evicted = enforce_budget(&s, &lib, &["new".to_string()]).unwrap();
        assert_eq!(evicted, vec!["old".to_string()]);
        assert!(!s.stems_dir("old", "m").exists());
        assert!(lib.separation("old", "m").unwrap().is_none());
        assert_eq!(lib.selected_source(old).unwrap().unwrap().status, crate::library::SourceStatus::Pending);
        assert!(s.stems_dir("gone", "m").exists());
    }

    #[test]
    fn startup_cleanup_removes_parts_and_recounts() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::new(dir.path());
        let lib = Library::open_in_memory().unwrap();
        write_chunk(&s, "h", "m", &chunk(0, 100)).unwrap();
        write_chunk(&s, "h", "m", &chunk(1, 100)).unwrap();
        std::fs::write(s.stems_dir("h", "m").join("0002.vocals.flac.part"), b"x").unwrap();
        std::fs::create_dir_all(s.tmp_dir()).unwrap();
        std::fs::write(s.tmp_dir().join("half.m4a"), b"x").unwrap();
        let mut row = sep_row("h", 4, SepStatus::Running, 0, 0);
        row.chunks_done = 3; // DB claimed 3, but only 2 pairs made it to disk
        lib.upsert_separation(&row).unwrap();
        // A song whose chunks all landed but which crashed before `finish`.
        write_chunk(&s, "done", "m", &chunk(0, 100)).unwrap();
        write_atomic(&s.source_path("done"), b"src").unwrap();
        lib.upsert_separation(&sep_row("done", 1, SepStatus::Running, 0, 0)).unwrap();

        startup_cleanup(&s, &lib).unwrap();

        assert!(!s.stems_dir("h", "m").join("0002.vocals.flac.part").exists());
        assert_eq!(std::fs::read_dir(s.tmp_dir()).unwrap().count(), 0);
        let row = lib.separation("h", "m").unwrap().unwrap();
        assert_eq!((row.chunks_done, row.status), (2, SepStatus::Queued));
        assert_eq!(lib.separation("done", "m").unwrap().unwrap().status, SepStatus::Ready);
        assert!(!s.source_path("done").exists());
    }
}
