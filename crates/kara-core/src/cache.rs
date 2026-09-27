//! The separated-audio cache: chunk files on disk, marking songs ready, the
//! disk budget (least recently played goes first) and cleanup after a crash.

use crate::audio::{decode_range, encode_flac, is_damaged, read_flac, Stereo, SAMPLE_RATE};
use crate::library::{Library, SepStatus, SourceKind};
use crate::problem::Problem;
use crate::separate::mdx::ChunkOut;
use crate::store::{write_atomic, DataLock, Store};
use anyhow::{Context, Result};
use std::path::Path;

pub const DEFAULT_BUDGET: u64 = 5 * 1024 * 1024 * 1024;

pub struct Entry {
    pub size_bytes: i64,
    pub last_used_at: i64,
    pub ready: bool,
    pub protected: bool,
}

/// Indices to evict, unfinished songs first, then least recently used, until
/// the total fits `budget`.
pub fn pick_evictions(entries: &[Entry], budget: u64) -> Vec<usize> {
    let mut total: i64 = entries.iter().map(|e| e.size_bytes).sum();
    let mut order: Vec<usize> = (0..entries.len()).filter(|&i| !entries[i].protected).collect();
    order.sort_by_key(|&i| (entries[i].ready, entries[i].last_used_at));
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

/// Vocals are stored at half level so peaks up to twice full scale fit.
const VOCALS_LEVEL: f32 = 0.5;

/// Writes one chunk's vocals.
pub fn write_chunk(store: &Store, hash: &str, model_id: &str, c: &ChunkOut) -> Result<()> {
    let scale = |x: &[f32]| x.iter().map(|s| s * VOCALS_LEVEL).collect();
    let vocals = Stereo { left: scale(&c.vocals.left), right: scale(&c.vocals.right) };
    write_atomic(&store.chunk_path(hash, model_id, c.index as u32), &encode_flac(&vocals)?)
}

fn read_vocals(store: &Store, hash: &str, model_id: &str, index: u32) -> Result<Stereo> {
    let mut v = read_flac(&store.chunk_path(hash, model_id, index))?;
    v.left.iter_mut().chain(v.right.iter_mut()).for_each(|x| *x /= VOCALS_LEVEL);
    Ok(v)
}

/// One chunk of both tracks as interleaved stereo f32 (L, R, L, R, …) at 44.1 kHz.
/// A full chunk holds `CHUNK_LEN` frames; the last one holds the rest of the song.
pub struct ChunkPcm {
    pub vocals: Vec<f32>,
    /// The original minus the vocals.
    pub inst: Vec<f32>,
}

fn interleave(a: &Stereo) -> Vec<f32> {
    a.left.iter().zip(&a.right).flat_map(|(l, r)| [*l, *r]).collect()
}

/// One chunk's vocals and instrumental, readable as soon as that chunk is written.
/// Deletes a damaged original, so the next prepare fetches the song again.
pub fn chunk_pcm(store: &Store, lib: &Library, hash: &str, model_id: &str, index: u32) -> Result<ChunkPcm> {
    let row = lib.separation(hash, model_id)?.context(Problem::NotPrepared)?;
    let original = store.original_path(hash).context(Problem::NotPrepared)?;
    let vocals = read_vocals(store, hash, model_id, index)?;
    let chunk_len = row.chunk_ms as usize * SAMPLE_RATE as usize / 1000;
    let mix = decode_range(&original, index as usize * chunk_len, vocals.len()).inspect_err(|e| {
        if is_damaged(e) {
            let _ = std::fs::remove_file(&original);
        }
    })?;
    let minus = |m: &[f32], v: &[f32]| m.iter().zip(v).map(|(m, v)| m - v).collect();
    let inst = Stereo { left: minus(&mix.left, &vocals.left), right: minus(&mix.right, &vocals.right) };
    Ok(ChunkPcm { vocals: interleave(&vocals), inst: interleave(&inst) })
}

/// `chunk_pcm` for a track's selected audio.
pub fn track_chunk_pcm(store: &Store, lib: &Library, model_id: &str, track_id: i64, index: u32) -> Result<ChunkPcm> {
    let hash = lib.selected_source(track_id)?.and_then(|s| s.audio_hash).context(Problem::NotPrepared)?;
    chunk_pcm(store, lib, &hash, model_id, index).context(Problem::PartNotReady)
}

/// Chunk files on disk, counting up from 0 and stopping at the first gap.
pub fn count_complete_chunks(store: &Store, hash: &str, model_id: &str) -> u32 {
    let mut n = 0;
    while store.chunk_path(hash, model_id, n).exists() {
        n += 1;
    }
    n
}

/// Total size of the files under `dir`, including subfolders.
fn dir_size(dir: &Path) -> i64 {
    let Ok(rd) = std::fs::read_dir(dir) else { return 0 };
    rd.filter_map(|e| e.ok())
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len() as i64,
            Err(_) => 0,
        })
        .sum()
}

/// All chunks are on disk: mark the song ready.
pub fn finish(lib: &Library, hash: &str, model_id: &str) -> Result<()> {
    let mut row = lib.separation(hash, model_id)?.context("missing separation row")?;
    row.status = SepStatus::Ready;
    row.chunks_done = row.chunks_total;
    row.last_used_at = Some(crate::now_ms());
    lib.upsert_separation(&row)
}

/// Removes a song from the library, any album or artist left empty, and its
/// audio folder unless another song shares the same audio.
pub fn delete_track(store: &Store, lib: &Library, track_id: i64) -> Result<()> {
    let hash = lib.selected_source(track_id)?.and_then(|s| s.audio_hash);
    lib.delete_track(track_id)?;
    lib.prune_empty_collections()?;
    let Some(hash) = hash else { return Ok(()) };
    if lib.sources_with_hash(&hash)?.is_empty() {
        let _ = std::fs::remove_dir_all(store.audio_dir(&hash));
        for r in lib.separations()?.iter().filter(|r| r.audio_hash == hash) {
            lib.delete_separation(&hash, &r.model_id)?;
        }
    }
    Ok(())
}

pub fn budget(lib: &Library) -> Result<u64> {
    Ok(lib.setting("cache_budget_bytes")?.and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_BUDGET))
}

/// Bytes the songs' audio takes on disk.
pub fn usage(store: &Store) -> u64 {
    dir_size(&store.audio_root()) as u64
}

/// Deletes whole songs' audio folders until the cache fits its budget:
/// unfinished songs first, then the least recently played. Never touches
/// `protected` hashes (playing / queued), or a song whose only source is a
/// local file that no longer exists.
pub fn enforce_budget(store: &Store, lib: &Library, protected: &[String]) -> Result<Vec<String>> {
    evict_to(store, lib, protected, budget(lib)?)
}

/// Deletes every song's prepared audio, with the same exceptions as `enforce_budget`.
pub fn clear(store: &Store, lib: &Library, protected: &[String]) -> Result<Vec<String>> {
    evict_to(store, lib, protected, 0)
}

/// A song folder's modified time, in milliseconds since the Unix epoch: when
/// its original landed, or "now" while it's still being written.
fn folder_age(store: &Store, hash: &str) -> i64 {
    std::fs::metadata(store.audio_dir(hash))
        .and_then(|m| m.modified())
        .ok()
        .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as i64)
}

fn evict_to(store: &Store, lib: &Library, protected: &[String], budget: u64) -> Result<Vec<String>> {
    let rows = lib.separations()?;
    let hashes: Vec<String> = match std::fs::read_dir(store.audio_root()) {
        Ok(rd) => rd
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
            .filter_map(|e| e.file_name().into_string().ok())
            .collect(),
        Err(_) => Vec::new(),
    };
    let mut entries = Vec::with_capacity(hashes.len());
    for hash in &hashes {
        let ready = rows.iter().find(|r| &r.audio_hash == hash && r.status == SepStatus::Ready);
        let started = rows.iter().any(|r| &r.audio_hash == hash);
        let unrecoverable = lib
            .sources_with_hash(hash)?
            .iter()
            .any(|s| s.kind == SourceKind::File && !Path::new(&s.uri).exists());
        entries.push(Entry {
            size_bytes: dir_size(&store.audio_dir(hash)),
            last_used_at: ready
                .and_then(|r| r.last_used_at)
                .unwrap_or_else(|| if started { 0 } else { folder_age(store, hash) }),
            ready: ready.is_some() || !started,
            protected: unrecoverable || protected.contains(hash),
        });
    }
    let mut evicted = Vec::new();
    for i in pick_evictions(&entries, budget) {
        let hash = &hashes[i];
        std::fs::remove_dir_all(store.audio_dir(hash))?;
        for r in rows.iter().filter(|r| &r.audio_hash == hash) {
            lib.delete_separation(hash, &r.model_id)?;
        }
        lib.reset_sources_for_hash(hash)?;
        evicted.push(hash.clone());
    }
    Ok(evicted)
}

/// Half-written files and chunk files of the old layout.
fn is_stale(p: &Path) -> bool {
    let name = p.file_name().unwrap_or_default().to_string_lossy();
    [".part", ".vocals.flac", ".inst.flac"].iter().any(|s| name.ends_with(s))
}

/// Removes the stale files under `dir`, skipping any it can't.
fn remove_stale_files(dir: &Path) {
    let Ok(rd) = std::fs::read_dir(dir) else { return };
    for p in rd.filter_map(|e| Some(e.ok()?.path())) {
        if p.is_dir() {
            remove_stale_files(&p);
        } else if is_stale(&p) {
            let _ = std::fs::remove_file(&p);
        }
    }
}

/// Run once at launch, holding the data folder: clear temp downloads,
/// half-written files and old chunk files, then make the database agree with the chunk files on disk.
pub fn startup_cleanup(store: &Store, lib: &Library, _lock: &DataLock) -> Result<()> {
    let _ = std::fs::remove_dir_all(store.tmp_dir());
    std::fs::create_dir_all(store.tmp_dir())?;
    remove_stale_files(&store.audio_root());
    for mut row in lib.separations()? {
        let done = count_complete_chunks(store, &row.audio_hash, &row.model_id);
        if done >= row.chunks_total {
            if row.status != SepStatus::Ready {
                finish(lib, &row.audio_hash, &row.model_id)?;
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
    use crate::library::{NewTrack, ProviderId, SeparationRow, SourceKind};
    use crate::separate::mdx::ChunkOut;

    fn e(size: i64, used: i64, protected: bool) -> Entry {
        Entry { size_bytes: size, last_used_at: used, ready: true, protected }
    }

    fn chunk(index: usize, len: usize) -> ChunkOut {
        ChunkOut { index, vocals: Stereo::silence(len) }
    }

    fn sep_row(hash: &str, total: u32, status: SepStatus, used: i64) -> SeparationRow {
        SeparationRow { audio_hash: hash.into(), model_id: "m".into(), chunk_ms: 10_000, chunks_total: total, chunks_done: total, status, last_used_at: Some(used) }
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
    fn complete_chunks_are_counted_up_to_the_first_gap() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::new(dir.path());
        for i in [0, 1, 3] {
            write_chunk(&s, "h", "m", &chunk(i, 100)).unwrap();
        }
        assert_eq!(count_complete_chunks(&s, "h", "m"), 2);
    }

    #[test]
    fn pcm_is_the_vocals_and_the_original_minus_them_and_nothing_clips() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::new(dir.path());
        let lib = Library::open_in_memory().unwrap();
        // A loud 48 kHz original with its two channels in opposite phase.
        let orig = s.original_dest("h", Some("wav"));
        std::fs::create_dir_all(orig.parent().unwrap()).unwrap();
        let spec = hound::WavSpec { channels: 2, sample_rate: 48_000, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
        let mut w = hound::WavWriter::create(&orig, spec).unwrap();
        for i in 0..120_000 {
            let x = ((i as f32 * 0.03).sin() * 0.99 * 32767.0) as i16;
            w.write_sample(x).unwrap();
            w.write_sample(-x).unwrap();
        }
        w.finalize().unwrap();
        let mix = crate::audio::decode_file(&orig).unwrap().audio;
        // Vocals louder than full scale and in opposite phase: the instrumental reaches about 2.5.
        for (index, start) in (0..mix.len()).step_by(44_100).enumerate() {
            let part = mix.slice(start, (start + 44_100).min(mix.len()));
            let loud = |x: &Vec<f32>| x.iter().map(|s| -1.5 * s).collect();
            let vocals = Stereo { left: loud(&part.left), right: loud(&part.right) };
            write_chunk(&s, "h", "m", &ChunkOut { index, vocals }).unwrap();
        }
        let row = SeparationRow { chunk_ms: 1_000, ..sep_row("h", 3, SepStatus::Running, 0) };
        lib.upsert_separation(&row).unwrap();

        let mut peak = 0f32;
        for i in 0..3 {
            let pcm = chunk_pcm(&s, &lib, "h", "m", i).unwrap();
            let start = i as usize * 44_100;
            let want = mix.slice(start, (start + 44_100).min(mix.len()));
            assert_eq!((pcm.vocals.len(), pcm.inst.len()), (want.len() * 2, want.len() * 2));
            for (k, (v, n)) in pcm.vocals.iter().zip(&pcm.inst).enumerate() {
                let m = if k % 2 == 0 { want.left[k / 2] } else { want.right[k / 2] };
                assert!((v + n - m).abs() <= 1.0 / 32767.0, "chunk {i} sample {k}");
                assert!((v + 1.5 * m).abs() <= 3.0 / 32767.0, "vocals changed at chunk {i} sample {k}");
                peak = peak.max(n.abs());
            }
        }
        assert!(peak > 2.4, "instrumental peak {peak}");
    }

    #[test]
    fn enforce_budget_evicts_partial_songs_first_and_keeps_protected_ones() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::new(dir.path());
        let lib = Library::open_in_memory().unwrap();
        lib.set_setting("cache_budget_bytes", "0").unwrap();
        let add = |hash: &str, kind, uri: &str| {
            let t = lib.add_track(&NewTrack { provider: ProviderId::Local, provider_ref: None, title: hash, artist: None, album: None, duration_ms: None }).unwrap();
            let src = lib.add_source(t, kind, uri, None).unwrap();
            lib.set_source_audio(src, hash, 1000).unwrap();
            t
        };
        // "old": ready, least recently used. "partial": cancelled halfway. "queued": partial but protected.
        // "gone": ready, but its original file was deleted. "new": ready and protected.
        let old = add("old", SourceKind::Link, "https://youtu.be/x");
        let partial = add("partial", SourceKind::Link, "https://youtu.be/y");
        add("queued", SourceKind::Link, "https://youtu.be/z");
        add("gone", SourceKind::File, "/nowhere/gone.wav");
        for (h, status, used) in [("old", SepStatus::Ready, 1), ("gone", SepStatus::Ready, 2), ("new", SepStatus::Ready, 3), ("partial", SepStatus::Cancelled, 4), ("queued", SepStatus::Cancelled, 5)] {
            write_chunk(&s, h, "m", &chunk(0, 10)).unwrap();
            lib.upsert_separation(&sep_row(h, 2, status, used)).unwrap();
        }
        write_atomic(&s.original_dest("partial", Some("m4a")), b"src").unwrap();

        let evicted = enforce_budget(&s, &lib, &["new".to_string(), "queued".to_string()]).unwrap();

        assert_eq!(evicted, vec!["partial".to_string(), "old".to_string()]);
        for (h, t) in [("partial", partial), ("old", old)] {
            assert!(!s.audio_dir(h).exists());
            assert!(lib.separation(h, "m").unwrap().is_none());
            assert_eq!(lib.selected_source(t).unwrap().unwrap().status, crate::library::SourceStatus::Pending);
        }
        for h in ["queued", "gone", "new"] {
            assert!(s.stems_dir(h, "m").exists());
        }
    }

    #[test]
    fn clearing_ignores_the_budget_but_keeps_protected_songs() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::new(dir.path());
        let lib = Library::open_in_memory().unwrap();
        for h in ["a", "b"] {
            write_chunk(&s, h, "m", &chunk(0, 10)).unwrap();
            lib.upsert_separation(&sep_row(h, 1, SepStatus::Ready, 1)).unwrap();
        }
        assert!(usage(&s) > 0);
        assert_eq!(clear(&s, &lib, &["b".to_string()]).unwrap(), vec!["a".to_string()]);
        assert!(!s.audio_dir("a").exists() && s.audio_dir("b").exists());
    }

    #[test]
    fn a_just_added_song_outlives_an_older_played_one() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::new(dir.path());
        let lib = Library::open_in_memory().unwrap();
        // "old": played long ago. "added": no separation row yet (just added).
        write_chunk(&s, "old", "m", &chunk(0, 10)).unwrap();
        lib.upsert_separation(&sep_row("old", 1, SepStatus::Ready, 1)).unwrap();
        write_chunk(&s, "added", "m", &chunk(0, 10)).unwrap();
        let budget = dir_size(&s.audio_dir("added"));
        lib.set_setting("cache_budget_bytes", &budget.to_string()).unwrap();

        assert_eq!(enforce_budget(&s, &lib, &[]).unwrap(), vec!["old".to_string()]);
        assert!(s.audio_dir("added").exists());
    }

    #[test]
    fn enforce_budget_ignores_stray_files_in_the_audio_folder() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::new(dir.path());
        let lib = Library::open_in_memory().unwrap();
        lib.set_setting("cache_budget_bytes", "0").unwrap();
        write_chunk(&s, "h", "m", &chunk(0, 10)).unwrap();
        lib.upsert_separation(&sep_row("h", 1, SepStatus::Ready, 1)).unwrap();
        std::fs::write(s.audio_root().join(".DS_Store"), b"x").unwrap();
        assert_eq!(enforce_budget(&s, &lib, &[]).unwrap(), vec!["h".to_string()]);
    }

    #[test]
    fn deleting_a_song_frees_its_audio_unless_another_song_shares_it() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::new(dir.path());
        let lib = Library::open_in_memory().unwrap();
        let add = |title: &str, hash: &str| {
            let t = lib.add_track(&NewTrack { provider: ProviderId::Local, provider_ref: None, title, artist: Some("Juniper Row"), album: None, duration_ms: None }).unwrap();
            crate::ingest::link_collections(&lib, t, Some("Juniper Row"), None).unwrap();
            let src = lib.add_source(t, SourceKind::Link, &format!("https://youtu.be/{title}"), None).unwrap();
            lib.set_source_audio(src, hash, 1000).unwrap();
            t
        };
        let (a, copy, other) = (add("a", "shared"), add("copy", "shared"), add("other", "own"));
        for h in ["shared", "own"] {
            write_chunk(&s, h, "m", &chunk(0, 10)).unwrap();
            lib.upsert_separation(&sep_row(h, 1, SepStatus::Ready, 1)).unwrap();
        }
        delete_track(&s, &lib, a).unwrap();
        assert!(s.audio_dir("shared").exists());
        delete_track(&s, &lib, copy).unwrap();
        assert!(!s.audio_dir("shared").exists());
        assert!(lib.separation("shared", "m").unwrap().is_none());
        assert_eq!(lib.collections(None, crate::library::CollectionKind::Artist).unwrap().len(), 1);
        delete_track(&s, &lib, other).unwrap();
        assert!(lib.collections(None, crate::library::CollectionKind::Artist).unwrap().is_empty());
        assert!(lib.track(a).is_err());
    }

    #[test]
    fn startup_cleanup_removes_parts_and_old_chunk_files_and_recounts() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::new(dir.path());
        let lib = Library::open_in_memory().unwrap();
        write_chunk(&s, "h", "m", &chunk(0, 100)).unwrap();
        write_chunk(&s, "h", "m", &chunk(1, 100)).unwrap();
        let part = s.stems_dir("h", "m").join("0002.flac.part");
        std::fs::write(&part, b"x").unwrap();
        let old_layout = ["0002.vocals.flac", "0002.inst.flac"].map(|n| s.stems_dir("h", "m").join(n));
        old_layout.iter().for_each(|p| std::fs::write(p, b"x").unwrap());
        std::fs::create_dir_all(s.tmp_dir()).unwrap();
        std::fs::write(s.tmp_dir().join("half.m4a"), b"x").unwrap();
        let mut row = sep_row("h", 4, SepStatus::Running, 0);
        row.chunks_done = 3; // DB claimed 3, but only 2 chunks made it to disk
        lib.upsert_separation(&row).unwrap();
        // A song whose chunks all landed but which crashed before `finish`.
        write_chunk(&s, "done", "m", &chunk(0, 100)).unwrap();
        lib.upsert_separation(&sep_row("done", 1, SepStatus::Running, 0)).unwrap();

        startup_cleanup(&s, &lib, &s.lock().unwrap()).unwrap();

        assert!(!part.exists());
        assert!(old_layout.iter().all(|p| !p.exists()));
        assert_eq!(std::fs::read_dir(s.tmp_dir()).unwrap().count(), 0);
        let row = lib.separation("h", "m").unwrap().unwrap();
        assert_eq!((row.chunks_done, row.status), (2, SepStatus::Queued));
        assert_eq!(lib.separation("done", "m").unwrap().unwrap().status, SepStatus::Ready);
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn startup_cleanup_skips_a_file_it_cannot_remove() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::new(dir.path());
        let lib = Library::open_in_memory().unwrap();
        let [locked, other] = ["a/original.m4a.part", "b/original.m4a.part"].map(|p| s.audio_root().join(p));
        for p in [&locked, &other] {
            write_atomic(p, b"x").unwrap();
        }
        crate::test_util::chflags("uchg", &locked);
        let cleaned = startup_cleanup(&s, &lib, &s.lock().unwrap());
        crate::test_util::chflags("nouchg", &locked);
        cleaned.unwrap();
        assert!(!other.exists());
    }
}
