//! Preparing a song for singing: fetch → standardize → separate, with lyrics
//! looked up alongside on their own thread. `Worker` runs one song at a time:
//! the one you're about to sing first, then the rest of the queue. Switching
//! songs cancels the old one.

use crate::audio::{self, Stereo, SAMPLE_RATE};
use crate::cache;
use crate::ingest;
use crate::library::{AudioSource, Library, LyricsSource, SepStatus, SeparationRow, SourceKind, SourceStatus, Track};
use crate::lyric_sync;
use crate::lyrics::{self, LyricsFetcher};
use crate::now_ms;
use crate::problem::Problem;
use crate::separate::mdx::{self, MdxParams, Outcome, VocalModel};
use crate::store::{copy_atomic, DataLock, Store};
use anyhow::{Context, Result};
use std::path::Path;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread::JoinHandle;

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Stage {
    Fetching,
    Standardizing,
    FindingLyrics,
    Separating,
}

#[derive(Clone, Debug, PartialEq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Event {
    Stage { track_id: i64, stage: Stage },
    Progress { track_id: i64, chunks_done: u32, chunks_total: u32 },
    /// Lyrics were saved, including "none found"; read them from the library.
    Lyrics { track_id: i64 },
    /// The lyric timing was lined up with the singing.
    LyricOffset { track_id: i64 },
    /// A newly added song's audio and lyrics are ready; it is separated when played.
    Added { track_id: i64 },
    Ready { track_id: i64 },
    Failed { track_id: i64, message: String, problem: Option<Problem> },
}

#[derive(Clone, Debug)]
pub struct Ctx {
    pub store: Store,
    pub model_id: String,
    pub params: MdxParams,
    pub chunk_len: usize,
}

/// "No lyrics" results are retried after a week.
const LYRICS_RETRY_MS: i64 = 7 * 24 * 3600 * 1000;
/// Lyrics are first lined up once this much of the song is separated.
const EARLY_SYNC_MS: usize = 90_000;

/// Prepares one song: fetches and standardizes its audio if needed, asks
/// `lyrics` for a lookup, then separates vocals. Emits progress on `emit`,
/// marks the source failed with a plain-language message on error, and
/// resets it to ready on success.
pub fn prepare(
    ctx: &Ctx,
    lib: &Library,
    model: &mut dyn VocalModel,
    lyrics: &LyricsLookup,
    track_id: i64,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Event),
) -> Result<Outcome> {
    let result = prepare_inner(ctx, lib, model, lyrics, track_id, cancel, emit);
    match &result {
        Ok(Outcome::Done) => {
            if let Ok(Some(src)) = lib.selected_source(track_id) {
                let _ = lib.set_source_status(src.id, SourceStatus::Ready, None);
            }
            emit(Event::Ready { track_id });
            let _ = sync_whole_song(ctx, lib, track_id, emit);
        }
        Err(e) => fail(lib, track_id, e, emit),
        Ok(Outcome::Cancelled) => {}
    }
    result
}

/// Marks the song's audio failed with the error's plain message and reports it.
fn fail(lib: &Library, track_id: i64, e: &anyhow::Error, emit: &mut dyn FnMut(Event)) {
    let message = e.to_string();
    if let Ok(Some(src)) = lib.selected_source(track_id) {
        let _ = lib.set_source_status(src.id, SourceStatus::Failed, Some(&message));
    }
    emit(Event::Failed { track_id, message, problem: crate::problem::problem(e) });
}

/// Gets a newly added song's audio onto disk in the standard format and looks up
/// its lyrics, without taking the vocals out. Emits `Added` when done.
pub fn add(ctx: &Ctx, lib: &Library, fetcher: &dyn LyricsFetcher, track_id: i64, emit: &mut dyn FnMut(Event)) -> Result<()> {
    let result = add_inner(ctx, lib, fetcher, track_id, emit);
    match &result {
        Ok(()) => emit(Event::Added { track_id }),
        Err(e) => fail(lib, track_id, e, emit),
    }
    result
}

fn add_inner(ctx: &Ctx, lib: &Library, fetcher: &dyn LyricsFetcher, track_id: i64, emit: &mut dyn FnMut(Event)) -> Result<()> {
    let track = lib.track(track_id).context(Problem::SongGone)?;
    let source = lib
        .selected_source(track_id)
        .context(Problem::SongGone)?
        .context(Problem::NoAudio)?;
    let separated = match &source.audio_hash {
        Some(hash) => is_ready(ctx, lib, hash)?,
        None => false,
    };
    if !separated {
        load_or_fetch(ctx, lib, &track, &source, emit)?;
    }
    emit(Event::Stage { track_id, stage: Stage::FindingLyrics });
    if refresh_lyrics(lib, track_id, fetcher).unwrap_or(false) {
        emit(Event::Lyrics { track_id });
    }
    Ok(())
}

/// Marked ready in the library, with the original and every chunk file still on disk.
fn is_ready(ctx: &Ctx, lib: &Library, hash: &str) -> Result<bool> {
    Ok(ctx.store.original_path(hash).is_some()
        && lib.separation(hash, &ctx.model_id)?.is_some_and(|r| {
        r.status == SepStatus::Ready && cache::count_complete_chunks(&ctx.store, hash, &ctx.model_id) >= r.chunks_total
    }))
}

/// Whether a lyrics lookup is worth doing: none tried yet, or the last try
/// found nothing and it's been a week.
fn lyrics_due(lib: &Library, track_id: i64) -> Result<bool> {
    Ok(match lib.lyrics(track_id)? {
        None => true,
        Some(l) => l.source == LyricsSource::None && now_ms() - l.fetched_at > LYRICS_RETRY_MS,
    })
}

/// Looks up and saves a song's lyrics if they are due; returns whether it saved.
/// A failed lookup saves nothing, so it is tried again next time.
fn refresh_lyrics(lib: &Library, track_id: i64, fetcher: &dyn LyricsFetcher) -> Result<bool> {
    if !lyrics_due(lib, track_id)? {
        return Ok(false);
    }
    let (src, lines) = look_up_lyrics(lib, track_id, fetcher)?;
    lib.set_lyrics(track_id, src, &lines, now_ms())?;
    Ok(true)
}

fn look_up_lyrics(lib: &Library, track_id: i64, fetcher: &dyn LyricsFetcher) -> Result<(LyricsSource, Vec<lyrics::Line>)> {
    let t = lib.track(track_id)?;
    lyrics::find(None, &t.title, t.artist.as_deref(), t.duration_ms.unwrap_or(0), fetcher)
}

/// Looks up a song's lyrics now, even when it has some or none were found lately; saves any it finds,
/// forgets the timing set for the old ones and lines the new ones up with the singing if the song is separated.
/// Returns whether it found any.
pub fn find_lyrics_again(ctx: &Ctx, lib: &Library, fetcher: &dyn LyricsFetcher, track_id: i64, emit: &mut dyn FnMut(Event)) -> Result<bool> {
    let (src, lines) = look_up_lyrics(lib, track_id, fetcher).context(Problem::LyricsLookup)?;
    if src == LyricsSource::None {
        return Ok(false);
    }
    lib.set_lyrics(track_id, src, &lines, now_ms())?;
    if lib.reset_lyric_offset(track_id)? {
        emit(Event::LyricOffset { track_id });
    }
    emit(Event::Lyrics { track_id });
    let _ = sync_whole_song(ctx, lib, track_id, emit);
    Ok(true)
}

/// Looks up lyrics on its own thread, newest request first.
pub struct LyricsLookup {
    pending: Arc<Pending>,
    handle: Option<JoinHandle<()>>,
}

#[derive(Default)]
struct Pending {
    requests: Mutex<Requests>,
    wake: Condvar,
}

#[derive(Default)]
struct Requests {
    /// Oldest first.
    track_ids: Vec<i64>,
    closed: bool,
}

impl Pending {
    /// The newest request, waiting for one; `None` once closed with none left.
    fn next(&self) -> Option<i64> {
        let mut r = self.requests.lock().unwrap();
        loop {
            if let Some(t) = r.track_ids.pop() {
                return Some(t);
            }
            if r.closed {
                return None;
            }
            r = self.wake.wait(r).unwrap();
        }
    }

    fn update(&self, f: impl FnOnce(&mut Requests)) {
        f(&mut self.requests.lock().unwrap());
        self.wake.notify_all();
    }
}

impl LyricsLookup {
    /// Sends `Event::Lyrics` on `events` each time it saves a song's lyrics, then
    /// lines them up with the singing if the song is already separated.
    pub fn spawn(ctx: &Ctx, fetcher: Box<dyn LyricsFetcher + Send>, events: mpsc::Sender<Event>) -> Result<Self> {
        let lib = Library::open(&ctx.store.db_path())?;
        let ctx = ctx.clone();
        let pending = Arc::new(Pending::default());
        let p = pending.clone();
        let handle = std::thread::spawn(move || {
            while let Some(track_id) = p.next() {
                if refresh_lyrics(&lib, track_id, fetcher.as_ref()).unwrap_or(false) {
                    let _ = events.send(Event::Lyrics { track_id });
                    let _ = sync_whole_song(&ctx, &lib, track_id, &mut |e| {
                        let _ = events.send(e);
                    });
                }
            }
        });
        Ok(Self { pending, handle: Some(handle) })
    }

    fn request(&self, track_id: i64) {
        self.pending.update(|r| {
            r.track_ids.retain(|&t| t != track_id);
            r.track_ids.push(track_id);
        });
    }

    /// Drops waiting requests for songs not in `keep`.
    fn retain(&self, keep: &[i64]) {
        self.pending.update(|r| r.track_ids.retain(|t| keep.contains(t)));
    }

    /// Waits for the lookups already asked for, then stops.
    pub fn finish(mut self) {
        self.pending.update(|r| r.closed = true);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

impl Drop for LyricsLookup {
    /// Stops after the lookup in progress, dropping the waiting ones.
    fn drop(&mut self) {
        self.pending.update(|r| {
            r.closed = true;
            r.track_ids.clear();
        });
    }
}

/// For audio that's already separated: marks it used.
fn use_separated(ctx: &Ctx, lib: &Library, hash: &str) -> Result<Outcome> {
    lib.touch_separation(hash, &ctx.model_id, now_ms())?;
    Ok(Outcome::Done)
}

fn prepare_inner(
    ctx: &Ctx,
    lib: &Library,
    model: &mut dyn VocalModel,
    lyrics: &LyricsLookup,
    track_id: i64,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Event),
) -> Result<Outcome> {
    let track = lib.track(track_id).context(Problem::SongGone)?;
    let source = lib
        .selected_source(track_id)
        .context(Problem::SongGone)?
        .context(Problem::NoAudio)?;

    if let Some(hash) = &source.audio_hash {
        if is_ready(ctx, lib, hash)? {
            lyrics.request(track_id);
            return use_separated(ctx, lib, hash);
        }
    }

    let (hash, mix) = load_or_fetch(ctx, lib, &track, &source, emit)?;
    lyrics.request(track_id);
    if is_ready(ctx, lib, &hash)? {
        return use_separated(ctx, lib, &hash);
    }
    if cancel.load(Ordering::Relaxed) {
        return Ok(Outcome::Cancelled);
    }
    separate_stage(ctx, lib, track_id, &hash, &mix, model, cancel, emit)
}

/// The song in the standard format, plus its hash. Decodes the kept original
/// when there is one; if it's missing or damaged (deleted), fetches the song and keeps a copy.
fn load_or_fetch(ctx: &Ctx, lib: &Library, track: &Track, source: &AudioSource, emit: &mut dyn FnMut(Event)) -> Result<(String, Stereo)> {
    if let Some(hash) = &source.audio_hash {
        if let Some(p) = ctx.store.original_path(hash) {
            match audio::decode_file(&p) {
                Ok(d) => return Ok((hash.clone(), d.audio)),
                Err(e) if audio::is_damaged(&e) => std::fs::remove_file(&p).context(Problem::SongAudio)?,
                Err(e) => return Err(e.context(Problem::SongAudio)),
            }
        }
    }
    emit(Event::Stage { track_id: track.id, stage: Stage::Fetching });
    lib.set_source_status(source.id, SourceStatus::Fetching, None)?;
    let path = ingest::fetch_audio(lib, &ctx.store, track, source)?;
    emit(Event::Stage { track_id: track.id, stage: Stage::Standardizing });
    let kept = decode_and_keep(ctx, &path);
    if source.kind == SourceKind::Link {
        let _ = std::fs::remove_file(&path);
    }
    let (hash, decoded) = kept?;
    lib.set_source_audio(source.id, &hash, decoded.audio.duration_ms())?;
    if let Some(p) = decoded.tags.picture.as_ref().filter(|_| track.artwork_path.is_none()) {
        if let Some(ext) = ingest::image_ext(&p.media_type) {
            ingest::save_artwork(lib, &ctx.store, track.id, &p.data, ext)?;
        }
    }
    if let Some(text) = decoded.tags.lyrics.as_deref().filter(|t| lyrics::is_synced(t)) {
        let lines = lyrics::parse_lrc(text, decoded.audio.duration_ms());
        lib.set_lyrics(track.id, LyricsSource::Embedded, &lines, now_ms())?;
        emit(Event::Lyrics { track_id: track.id });
    }
    Ok((hash, decoded.audio))
}

/// Decodes a fetched file and keeps a copy of it as the song's original.
fn decode_and_keep(ctx: &Ctx, path: &Path) -> Result<(String, audio::Decoded)> {
    let decoded = audio::decode_file(path).map_err(|e| audio::describe_read_failure(e, Problem::Unreadable))?;
    anyhow::ensure!(!decoded.audio.is_empty(), Problem::Empty);
    let hash = audio::audio_hash(&decoded.audio);
    if ctx.store.original_path(&hash).is_none() {
        let dst = ctx.store.original_dest(&hash, path.extension().and_then(|e| e.to_str()));
        copy_atomic(path, &dst).map_err(|e| {
            let full = e.downcast_ref::<std::io::Error>().is_some_and(|e| e.kind() == std::io::ErrorKind::StorageFull);
            e.context(if full { Problem::DiskFull } else { Problem::Save })
        })?;
    }
    Ok((hash, decoded))
}

/// Separates `mix` from its first missing chunk, saving each finished chunk
/// and reporting progress as it goes.
#[allow(clippy::too_many_arguments)]
fn separate_stage(
    ctx: &Ctx,
    lib: &Library,
    track_id: i64,
    hash: &str,
    mix: &Stereo,
    model: &mut dyn VocalModel,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Event),
) -> Result<Outcome> {
    emit(Event::Stage { track_id, stage: Stage::Separating });
    let total = mdx::chunk_count(mix.len(), ctx.chunk_len) as u32;
    let start = cache::count_complete_chunks(&ctx.store, hash, &ctx.model_id);
    let mut row = lib.separation(hash, &ctx.model_id)?.unwrap_or(SeparationRow {
        audio_hash: hash.to_string(),
        model_id: ctx.model_id.clone(),
        chunk_ms: (ctx.chunk_len as u64 * 1000 / SAMPLE_RATE as u64) as u32,
        chunks_total: total,
        chunks_done: 0,
        status: SepStatus::Queued,
        last_used_at: Some(now_ms()),
    });
    row.chunks_done = start;
    row.status = SepStatus::Running;
    lib.upsert_separation(&row)?;
    emit(Event::Progress { track_id, chunks_done: start, chunks_total: total });

    let early_sync = (EARLY_SYNC_MS * SAMPLE_RATE as usize / 1000).div_ceil(ctx.chunk_len) as u32;
    let outcome = mdx::separate(model, &ctx.params, mix, ctx.chunk_len, start as usize, cancel, |chunk| {
        cache::write_chunk(&ctx.store, hash, &ctx.model_id, &chunk)?;
        row.chunks_done = chunk.index as u32 + 1;
        lib.upsert_separation(&row)?;
        emit(Event::Progress { track_id, chunks_done: row.chunks_done, chunks_total: total });
        if row.chunks_done == early_sync && early_sync < total {
            let _ = sync_lyrics(ctx, lib, track_id, hash, early_sync, emit);
        }
        Ok(())
    });
    let outcome = match outcome {
        Ok(o) => o,
        Err(e) => {
            row.status = SepStatus::Failed;
            let _ = lib.upsert_separation(&row);
            return Err(e.context(Problem::Separate));
        }
    };

    match outcome {
        Outcome::Cancelled => {
            row.status = SepStatus::Cancelled;
            lib.upsert_separation(&row)?;
        }
        Outcome::Done => {
            cache::finish(lib, hash, &ctx.model_id)?;
        }
    }
    Ok(outcome)
}

/// Lines the song's lyrics up with the singing in its first `chunks` of vocals,
/// unless the user set the timing, and reports a change.
fn sync_lyrics(ctx: &Ctx, lib: &Library, track_id: i64, hash: &str, chunks: u32, emit: &mut dyn FnMut(Event)) -> Result<()> {
    let Some(source) = lib.selected_source(track_id)?.filter(|s| !s.lyric_offset_manual) else { return Ok(()) };
    let Some(lyrics) = lib.lyrics(track_id)?.filter(|l| !l.lines.is_empty()) else { return Ok(()) };
    let sung = lyric_sync::singing(&lyric_sync::vocal_loudness(&ctx.store, hash, &ctx.model_id, chunks)?);
    if let Some(ms) = lyric_sync::best_offset(&sung, &lyric_sync::lyric_activity(&lyrics.lines)) {
        if lib.set_auto_lyric_offset(source.id, ms)? {
            emit(Event::LyricOffset { track_id });
        }
    }
    Ok(())
}

/// Lines a separated song's lyrics up with all of its singing, once per saved lyrics.
fn sync_whole_song(ctx: &Ctx, lib: &Library, track_id: i64, emit: &mut dyn FnMut(Event)) -> Result<()> {
    let Some(source) = lib.selected_source(track_id)? else { return Ok(()) };
    let Some(hash) = source.audio_hash.as_deref().filter(|h| is_ready(ctx, lib, h).unwrap_or(false)) else { return Ok(()) };
    let Some(lyrics) = lib.lyrics(track_id)?.filter(|l| source.lyric_synced_at != Some(l.fetched_at)) else { return Ok(()) };
    let chunks = cache::count_complete_chunks(&ctx.store, hash, &ctx.model_id);
    sync_lyrics(ctx, lib, track_id, hash, chunks, emit)?;
    lib.set_lyric_synced_at(source.id, lyrics.fetched_at)
}

/// Adds songs one at a time on its own thread.
pub struct Adder {
    requests: mpsc::Sender<i64>,
}

impl Adder {
    pub fn spawn(ctx: Ctx, fetcher: Box<dyn LyricsFetcher + Send>, events: mpsc::Sender<Event>) -> Result<Self> {
        let lib = Library::open(&ctx.store.db_path())?;
        let (requests, rx) = mpsc::channel::<i64>();
        std::thread::spawn(move || {
            for track_id in rx {
                let _ = add(&ctx, &lib, fetcher.as_ref(), track_id, &mut |e| {
                    let _ = events.send(e);
                });
            }
        });
        Ok(Self { requests })
    }

    pub fn add(&self, track_id: i64) {
        let _ = self.requests.send(track_id);
    }
}

// ---------------------------------------------------------------- worker

struct State {
    queue: VecDeque<i64>,
    running: Option<(i64, Arc<AtomicBool>)>,
    /// `tracks[0]` from the latest `play()` call: never evicted, regardless
    /// of where the queue or the running job currently are.
    playing: Option<i64>,
    shutdown: bool,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

pub struct Worker {
    shared: Arc<Shared>,
    lyrics: Arc<LyricsLookup>,
    handle: Option<JoinHandle<()>>,
    _lock: DataLock,
}

impl Worker {
    /// Starts a background thread that prepares queued songs one at a time
    /// and reports their events on `events`. Holds `lock` while it lives.
    pub fn spawn(
        ctx: Ctx,
        lock: DataLock,
        mut model: Box<dyn VocalModel>,
        fetcher: Box<dyn LyricsFetcher + Send>,
        events: mpsc::Sender<Event>,
    ) -> Result<Self> {
        let lib = Library::open(&ctx.store.db_path())?;
        let lyrics = Arc::new(LyricsLookup::spawn(&ctx, fetcher, events.clone())?);
        let shared = Arc::new(Shared { state: Mutex::new(State { queue: VecDeque::new(), running: None, playing: None, shutdown: false }), wake: Condvar::new() });
        let (s, l) = (shared.clone(), lyrics.clone());
        let handle = std::thread::spawn(move || {
            loop {
                let (track, flag) = {
                    let mut st = s.state.lock().unwrap();
                    loop {
                        if st.shutdown {
                            return;
                        }
                        if let Some(t) = st.queue.pop_front() {
                            let f = Arc::new(AtomicBool::new(false));
                            st.running = Some((t, f.clone()));
                            break (t, f);
                        }
                        st = s.wake.wait(st).unwrap();
                    }
                };
                let _ = prepare(&ctx, &lib, model.as_mut(), &l, track, &flag, &mut |e| {
                    let _ = events.send(e);
                });
                let (upcoming, playing): (Vec<i64>, Option<i64>) = {
                    let mut st = s.state.lock().unwrap();
                    st.running = None;
                    (st.queue.iter().copied().collect(), st.playing)
                };
                let protected = cache::audio_hashes(&lib, std::iter::once(track).chain(playing).chain(upcoming));
                let _ = cache::enforce_budget(&ctx.store, &lib, &protected);
            }
        });
        Ok(Self { shared, lyrics, handle: Some(handle), _lock: lock })
    }

    /// `tracks[0]` is the song to prepare now; the rest are prepared next, in order.
    /// A running job for any other song is cancelled.
    pub fn play(&self, tracks: Vec<i64>) {
        self.lyrics.retain(&tracks);
        let mut st = self.shared.state.lock().unwrap();
        st.playing = tracks.first().copied();
        let mut queue: VecDeque<i64> = tracks.into();
        if let Some((running, flag)) = &st.running {
            if queue.front() == Some(running) {
                // Already preparing this song: don't queue it again, unless
                // its job was told to cancel (then it must run again).
                if !flag.load(Ordering::Relaxed) {
                    queue.pop_front();
                }
            } else {
                flag.store(true, Ordering::Relaxed);
            }
        }
        st.queue = queue;
        self.shared.wake.notify_all();
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        {
            let mut st = self.shared.state.lock().unwrap();
            st.shutdown = true;
            if let Some((_, flag)) = &st.running {
                flag.store(true, Ordering::Relaxed);
            }
        }
        self.shared.wake.notify_all();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest;
    use crate::library::SourceStatus;
    use crate::test_util::{test_params, write_sine_wav, Silence};
    use std::path::Path;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    struct NoLyrics;
    impl LyricsFetcher for NoLyrics {
        fn fetch(&self, _: &str, _: Option<&str>, _: u64) -> Result<Option<String>> {
            Ok(None)
        }
    }

    struct MadeUpLyrics;
    impl LyricsFetcher for MadeUpLyrics {
        fn fetch(&self, _: &str, _: Option<&str>, _: u64) -> Result<Option<String>> {
            Ok(Some("[00:00.00]la la la\n".into()))
        }
    }

    /// Slow enough (2 ms/segment, hundreds of segments per chunk) that a
    /// cancel set right after `play()` lands well before it finishes.
    struct Slow;
    impl VocalModel for Slow {
        fn infer(&mut self, i: ndarray::Array4<f32>) -> Result<ndarray::Array4<f32>> {
            std::thread::sleep(Duration::from_millis(2));
            Ok(ndarray::Array4::zeros(i.dim()))
        }
    }

    fn ctx(root: &Path) -> Ctx {
        Ctx { store: Store::new(root), model_id: "test".into(), params: test_params(), chunk_len: 44_100 }
    }

    /// A 2.5 s song = 3 chunks of 1 s.
    fn song(dir: &Path, name: &str) -> std::path::PathBuf {
        let p = dir.join(name);
        write_sine_wav(&p, 44_100, 2, 2.5, 330.0);
        p
    }

    fn lookup(c: &Ctx, fetcher: impl LyricsFetcher + Send + 'static) -> (LyricsLookup, mpsc::Receiver<Event>) {
        let (tx, rx) = mpsc::channel();
        (LyricsLookup::spawn(c, Box::new(fetcher), tx).unwrap(), rx)
    }

    fn worker(c: &Ctx, model: impl VocalModel + 'static) -> (Worker, mpsc::Receiver<Event>) {
        let (tx, rx) = mpsc::channel();
        (Worker::spawn(c.clone(), c.store.lock().unwrap(), Box::new(model), Box::new(NoLyrics), tx).unwrap(), rx)
    }

    fn run(c: &Ctx, lib: &Library, track: i64, cancel: &AtomicBool) -> (Result<Outcome>, Vec<Event>) {
        let (lyrics, _) = lookup(c, NoLyrics);
        let mut events = Vec::new();
        let r = prepare(c, lib, &mut Silence, &lyrics, track, cancel, &mut |e| events.push(e));
        lyrics.finish();
        (r, events)
    }

    #[test]
    fn prepares_a_local_file_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let file = song(dir.path(), "a.wav");
        let t = ingest::add_file(&lib, &file).unwrap().track_id;
        let (r, events) = run(&c, &lib, t, &AtomicBool::new(false));
        assert_eq!(r.unwrap(), Outcome::Done);
        assert_eq!(events.first(), Some(&Event::Stage { track_id: t, stage: Stage::Fetching }));
        assert!(events.contains(&Event::Progress { track_id: t, chunks_done: 3, chunks_total: 3 }));
        assert_eq!(events.last(), Some(&Event::Ready { track_id: t }));
        let hash = lib.selected_source(t).unwrap().unwrap().audio_hash.unwrap();
        assert_eq!(lib.separation(&hash, "test").unwrap().unwrap().status, SepStatus::Ready);
        assert_eq!(lib.lyrics(t).unwrap().unwrap().source, LyricsSource::None);
        // Second time, even with the user's file moved away: instant, and it still plays.
        std::fs::remove_file(&file).unwrap();
        let (_, again) = run(&c, &lib, t, &AtomicBool::new(false));
        assert_eq!(again, vec![Event::Ready { track_id: t }]);
        assert!(cache::track_chunk_pcm(&c.store, &lib, "test", t, 2).is_ok());
    }

    #[test]
    fn chunk_audio_is_readable_by_track_at_its_exact_length_while_separating() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let t = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        let mut lens = Vec::new();
        prepare(&c, &lib, &mut Silence, &lookup(&c, NoLyrics).0, t, &AtomicBool::new(false), &mut |e| {
            if let Event::Progress { chunks_done: n @ 1.., .. } = e {
                let pcm = cache::track_chunk_pcm(&c.store, &lib, "test", t, n - 1).unwrap();
                lens.push((pcm.vocals.len(), pcm.inst.len()));
            }
        })
        .unwrap();
        // Two full 1 s chunks, then the last 0.5 s; two samples per frame.
        assert_eq!(lens, vec![(88_200, 88_200), (88_200, 88_200), (44_100, 44_100)]);
    }

    #[test]
    fn prepare_resumes_after_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let t = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        let cancel = AtomicBool::new(false);
        let r = prepare(&c, &lib, &mut Silence, &lookup(&c, NoLyrics).0, t, &cancel, &mut |e| {
            if let Event::Progress { chunks_done: 1, .. } = e {
                cancel.store(true, Ordering::Relaxed);
            }
        });
        assert_eq!(r.unwrap(), Outcome::Cancelled);
        let (r, events) = run(&c, &lib, t, &AtomicBool::new(false));
        assert_eq!(r.unwrap(), Outcome::Done);
        let first_progress = events.iter().find_map(|e| match e {
            Event::Progress { chunks_done, .. } => Some(*chunks_done),
            _ => None,
        });
        assert_eq!(first_progress, Some(1)); // picked up after the one finished chunk
        assert!(!events.iter().any(|e| matches!(e, Event::Stage { stage: Stage::Fetching, .. })));
    }

    #[test]
    fn same_audio_is_separated_once() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let t1 = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        let t2 = ingest::add_file(&lib, &song(dir.path(), "copy.wav")).unwrap().track_id;
        run(&c, &lib, t1, &AtomicBool::new(false)).0.unwrap();
        let (_, events) = run(&c, &lib, t2, &AtomicBool::new(false));
        assert!(!events.iter().any(|e| matches!(e, Event::Progress { .. })));
        assert_eq!(events.last(), Some(&Event::Ready { track_id: t2 }));
        assert_eq!(std::fs::read_dir(c.store.audio_root()).unwrap().count(), 1);
    }

    #[test]
    fn a_ready_song_missing_a_chunk_or_its_original_is_prepared_again() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let t = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        run(&c, &lib, t, &AtomicBool::new(false)).0.unwrap();
        let hash = lib.selected_source(t).unwrap().unwrap().audio_hash.unwrap();
        let lost = c.store.chunk_path(&hash, "test", 2);
        std::fs::remove_file(&lost).unwrap();
        let (_, events) = run(&c, &lib, t, &AtomicBool::new(false));
        assert!(events.contains(&Event::Progress { track_id: t, chunks_done: 3, chunks_total: 3 }));
        assert!(lost.exists());

        std::fs::remove_file(c.store.original_path(&hash).unwrap()).unwrap();
        let (_, events) = run(&c, &lib, t, &AtomicBool::new(false));
        assert!(events.contains(&Event::Stage { track_id: t, stage: Stage::Fetching }));
        assert!(c.store.original_path(&hash).is_some());
    }

    #[test]
    fn a_damaged_original_is_fetched_again() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let t = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        run(&c, &lib, t, &AtomicBool::new(false)).0.unwrap();
        let hash = lib.selected_source(t).unwrap().unwrap().audio_hash.unwrap();
        let damage = || std::fs::write(c.store.original_path(&hash).unwrap(), b"damaged").unwrap();
        let fetches_and_plays = || {
            let (r, events) = run(&c, &lib, t, &AtomicBool::new(false));
            assert_eq!(r.unwrap(), Outcome::Done);
            assert!(events.contains(&Event::Stage { track_id: t, stage: Stage::Fetching }));
            assert!(cache::track_chunk_pcm(&c.store, &lib, "test", t, 2).is_ok());
        };

        damage();
        assert!(cache::track_chunk_pcm(&c.store, &lib, "test", t, 0).is_err());
        fetches_and_plays();

        damage();
        std::fs::remove_file(c.store.chunk_path(&hash, "test", 2)).unwrap();
        fetches_and_plays();
    }

    #[cfg(unix)]
    #[test]
    fn an_original_that_cannot_be_opened_is_kept() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let t = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        run(&c, &lib, t, &AtomicBool::new(false)).0.unwrap();
        let hash = lib.selected_source(t).unwrap().unwrap().audio_hash.unwrap();
        let original = c.store.original_path(&hash).unwrap();
        std::fs::remove_file(c.store.chunk_path(&hash, "test", 2)).unwrap();
        std::fs::set_permissions(&original, std::fs::Permissions::from_mode(0o000)).unwrap();

        assert!(cache::track_chunk_pcm(&c.store, &lib, "test", t, 0).is_err());
        assert!(run(&c, &lib, t, &AtomicBool::new(false)).0.is_err());
        assert!(original.exists());
    }

    #[cfg(unix)]
    #[test]
    fn a_source_file_we_are_not_allowed_to_read_says_so() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let p = song(dir.path(), "a.wav");
        let t = ingest::add_file(&lib, &p).unwrap().track_id;
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o000)).unwrap();

        let (r, events) = run(&c, &lib, t, &AtomicBool::new(false));
        assert!(r.is_err());
        assert_eq!(events.last(), Some(&Event::Failed { track_id: t, message: "KaraAlwaysOK isn't allowed to read this file.".into(), problem: Some(Problem::FileNotAllowed) }));
    }

    #[test]
    fn a_missing_file_fails_with_a_plain_message() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let p = song(dir.path(), "a.wav");
        let t = ingest::add_file(&lib, &p).unwrap().track_id;
        std::fs::remove_file(&p).unwrap();
        let (r, events) = run(&c, &lib, t, &AtomicBool::new(false));
        assert!(r.is_err());
        assert_eq!(events.last(), Some(&Event::Failed { track_id: t, message: "The file was moved or deleted.".into(), problem: Some(Problem::FileMoved) }));
        let s = lib.selected_source(t).unwrap().unwrap();
        assert_eq!((s.status, s.error.as_deref()), (SourceStatus::Failed, Some("The file was moved or deleted.")));
    }

    #[test]
    fn a_save_failure_that_is_not_a_full_disk_does_not_blame_the_disk() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let t = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        std::fs::write(c.store.audio_root(), b"not a folder").unwrap();
        let (_, events) = run(&c, &lib, t, &AtomicBool::new(false));
        assert_eq!(events.last(), Some(&Event::Failed { track_id: t, message: "Couldn't save the audio.".into(), problem: Some(Problem::Save) }));
    }

    #[test]
    fn an_unknown_track_fails_with_a_plain_message() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let (r, events) = run(&c, &lib, 999, &AtomicBool::new(false));
        assert!(r.is_err());
        assert_eq!(events.last(), Some(&Event::Failed { track_id: 999, message: "This song is no longer in your library.".into(), problem: Some(Problem::SongGone) }));
    }

    #[test]
    fn separation_error_marks_the_row_failed_and_a_retry_recovers() {
        struct ErrOnSecondCall(u32);
        impl VocalModel for ErrOnSecondCall {
            fn infer(&mut self, i: ndarray::Array4<f32>) -> Result<ndarray::Array4<f32>> {
                self.0 += 1;
                if self.0 == 2 {
                    anyhow::bail!("boom");
                }
                Ok(ndarray::Array4::zeros(i.dim()))
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let t = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        let r = prepare(&c, &lib, &mut ErrOnSecondCall(0), &lookup(&c, NoLyrics).0, t, &AtomicBool::new(false), &mut |_| {});
        assert!(r.is_err());
        let hash = lib.selected_source(t).unwrap().unwrap().audio_hash.unwrap();
        assert_eq!(lib.separation(&hash, "test").unwrap().unwrap().status, SepStatus::Failed);

        let mut status_at_ready = None;
        let r = prepare(&c, &lib, &mut Silence, &lookup(&c, NoLyrics).0, t, &AtomicBool::new(false), &mut |e| {
            if let Event::Ready { .. } = e {
                status_at_ready = Some(lib.selected_source(t).unwrap().unwrap().status);
            }
        });
        assert_eq!(r.unwrap(), Outcome::Done);
        assert_eq!(status_at_ready, Some(SourceStatus::Ready)); // not the stale Failed from the first attempt
        let s = lib.selected_source(t).unwrap().unwrap();
        assert_eq!((s.status, s.error.as_deref()), (SourceStatus::Ready, None));
    }

    #[test]
    fn ready_does_not_wait_for_the_lyrics_lookup_and_lyrics_follow() {
        /// Finds made-up lyrics once the test opens the gate.
        struct Gated(mpsc::Receiver<()>);
        impl LyricsFetcher for Gated {
            fn fetch(&self, _: &str, _: Option<&str>, _: u64) -> Result<Option<String>> {
                self.0.recv_timeout(Duration::from_secs(10))?;
                Ok(Some("[00:00.00]la la la\n".into()))
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let t = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        lib.update_track_meta(t, "Title", Some("Artist"), None).unwrap();
        let (gate, gated) = mpsc::channel();
        let (lyrics, rx) = lookup(&c, Gated(gated));
        let started = std::time::Instant::now();
        let mut ready_after = None;
        prepare(&c, &lib, &mut Silence, &lyrics, t, &AtomicBool::new(false), &mut |e| {
            if let Event::Ready { .. } = e {
                ready_after = Some(started.elapsed());
            }
        })
        .unwrap();
        assert!(ready_after.unwrap() < Duration::from_secs(5), "Ready after {:?}", ready_after.unwrap());
        gate.send(()).unwrap();
        assert_eq!(rx.recv_timeout(Duration::from_secs(10)).unwrap(), Event::Lyrics { track_id: t });
        assert_eq!(lib.lyrics(t).unwrap().unwrap().lines[0].text, "la la la");
    }

    #[test]
    fn lyrics_are_retried_for_an_already_ready_song() {
        struct Offline;
        impl LyricsFetcher for Offline {
            fn fetch(&self, _: &str, _: Option<&str>, _: u64) -> Result<Option<String>> {
                anyhow::bail!("offline")
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let t = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        lib.update_track_meta(t, "Title", Some("Artist"), None).unwrap();
        let (lyrics, _) = lookup(&c, Offline);
        prepare(&c, &lib, &mut Silence, &lyrics, t, &AtomicBool::new(false), &mut |_| {}).unwrap();
        lyrics.finish();
        assert!(lib.lyrics(t).unwrap().is_none());

        let (lyrics, _) = lookup(&c, MadeUpLyrics);
        prepare(&c, &lib, &mut Silence, &lyrics, t, &AtomicBool::new(false), &mut |_| {}).unwrap();
        lyrics.finish();
        assert_eq!(lib.lyrics(t).unwrap().unwrap().lines[0].text, "la la la");
    }

    /// A FLAC with a made-up synced "lyrics" tag, built with ffmpeg.
    fn flac_with_embedded_lyrics(dir: &Path, name: &str) -> std::path::PathBuf {
        let p = dir.join(name);
        let status = std::process::Command::new("ffmpeg")
            .args([
                "-y",
                "-loglevel",
                "error",
                "-f",
                "lavfi",
                "-i",
                "sine=frequency=330:duration=2.5:sample_rate=44100",
                "-ac",
                "2",
                "-metadata",
                "lyrics=[00:00.00]embedded made up line\n",
            ])
            .arg(&p)
            .status()
            .expect("this test needs ffmpeg to build a tagged fixture file");
        assert!(status.success());
        p
    }

    #[test]
    fn embedded_lyrics_are_kept_and_the_fetcher_is_not_called() {
        struct CountingFetcher(Arc<std::sync::atomic::AtomicU32>);
        impl LyricsFetcher for CountingFetcher {
            fn fetch(&self, _: &str, _: Option<&str>, _: u64) -> Result<Option<String>> {
                self.0.fetch_add(1, Ordering::Relaxed);
                Ok(Some("[00:00.00]other made up text\n".into()))
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let t = ingest::add_file(&lib, &flac_with_embedded_lyrics(dir.path(), "a.flac")).unwrap().track_id;
        lib.update_track_meta(t, "Title", Some("Artist"), None).unwrap();
        let calls = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let (lyrics, _) = lookup(&c, CountingFetcher(calls.clone()));
        let mut events = Vec::new();
        prepare(&c, &lib, &mut Silence, &lyrics, t, &AtomicBool::new(false), &mut |e| events.push(e)).unwrap();
        lyrics.finish();
        assert!(events.contains(&Event::Lyrics { track_id: t }));
        let lyr = lib.lyrics(t).unwrap().unwrap();
        assert_eq!((lyr.source, lyr.lines[0].text.as_str()), (LyricsSource::Embedded, "embedded made up line"));
        assert_eq!(calls.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn lyrics_are_looked_up_on_the_shared_audio_fast_path() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let t1 = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        let t2 = ingest::add_file(&lib, &song(dir.path(), "copy.wav")).unwrap().track_id;
        lib.update_track_meta(t2, "Title", Some("Artist"), None).unwrap();
        run(&c, &lib, t1, &AtomicBool::new(false)).0.unwrap();
        let (lyrics, _) = lookup(&c, MadeUpLyrics);
        prepare(&c, &lib, &mut Silence, &lyrics, t2, &AtomicBool::new(false), &mut |_| {}).unwrap();
        lyrics.finish();
        assert_eq!(lib.lyrics(t2).unwrap().unwrap().lines[0].text, "la la la");
    }

    fn wait_for(rx: &mpsc::Receiver<Event>, want: &Event) -> Vec<Event> {
        let mut seen = Vec::new();
        loop {
            let e = rx.recv_timeout(Duration::from_secs(20)).expect("timed out waiting for event");
            let done = &e == want;
            seen.push(e);
            if done {
                return seen;
            }
        }
    }

    #[test]
    fn adding_gets_the_audio_and_lyrics_ready_without_taking_the_vocals_out() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let t = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        lib.update_track_meta(t, "Title", Some("Artist"), None).unwrap();
        let mut events = Vec::new();
        add(&c, &lib, &MadeUpLyrics, t, &mut |e| events.push(e)).unwrap();
        assert_eq!(
            events,
            vec![
                Event::Stage { track_id: t, stage: Stage::Fetching },
                Event::Stage { track_id: t, stage: Stage::Standardizing },
                Event::Stage { track_id: t, stage: Stage::FindingLyrics },
                Event::Lyrics { track_id: t },
                Event::Added { track_id: t },
            ]
        );
        let hash = lib.selected_source(t).unwrap().unwrap().audio_hash.unwrap();
        assert!(lib.separation(&hash, "test").unwrap().is_none());
        assert_eq!(lib.lyrics(t).unwrap().unwrap().lines[0].text, "la la la");
        let (_, events) = run(&c, &lib, t, &AtomicBool::new(false));
        assert_eq!(events.first(), Some(&Event::Stage { track_id: t, stage: Stage::Separating }));
    }

    #[test]
    fn the_adder_reports_a_song_it_cannot_read() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let p = song(dir.path(), "a.wav");
        let t = ingest::add_file(&lib, &p).unwrap().track_id;
        std::fs::remove_file(&p).unwrap();
        let (tx, rx) = mpsc::channel();
        let adder = Adder::spawn(c, Box::new(NoLyrics), tx).unwrap();
        adder.add(t);
        wait_for(&rx, &Event::Failed { track_id: t, message: "The file was moved or deleted.".into(), problem: Some(Problem::FileMoved) });
    }

    struct Lrc(String);
    impl LyricsFetcher for Lrc {
        fn fetch(&self, _: &str, _: Option<&str>, _: u64) -> Result<Option<String>> {
            Ok(Some(self.0.clone()))
        }
    }

    /// A separated two-minute song whose vocals sing its made-up LRC lyrics 1.5 s late; returns (track, source, LRC).
    fn separated_singing_song(c: &Ctx, lib: &Library) -> (i64, i64, String) {
        let t = titled(lib, "Paper Lanterns");
        let s = lib.add_source(t, SourceKind::File, "/music/a.wav", None).unwrap();
        lib.set_source_audio(s, "h", 120_000).unwrap();
        let mut lrc = String::new();
        let mut at = 4_000;
        for i in 0..60i64 {
            lrc += &format!("[{:02}:{:02}.{:02}]la la la\n", at / 60_000, at / 1_000 % 60, at / 10 % 100);
            at += 1_400 + (i * i * 37 % 23) * 100 + (i * i * 53 % 29) * 100;
        }
        let lines = lyrics::parse_lrc(&lrc, 120_000);
        let sung = |ms: i64| lines.iter().any(|l| (l.words[0].start_ms + 1_500..l.words[2].end_ms + 1_500).contains(&ms));
        for index in 0..12 {
            let samples: Vec<f32> = (0..441_000usize)
                .map(|i| {
                    let ms = (index * 441_000 + i) as i64 * 1_000 / 44_100;
                    (i as f32 * 0.03).sin() * if sung(ms) { 0.3 } else { 0.003 }
                })
                .collect();
            let vocals = Stereo { left: samples.clone(), right: samples };
            cache::write_chunk(&c.store, "h", "test", &mdx::ChunkOut { index, vocals }).unwrap();
        }
        std::fs::write(c.store.original_dest("h", Some("wav")), b"").unwrap();
        let row = SeparationRow { audio_hash: "h".into(), model_id: "test".into(), chunk_ms: 10_000, chunks_total: 12, chunks_done: 12, status: SepStatus::Ready, last_used_at: None };
        lib.upsert_separation(&row).unwrap();
        (t, s, lrc)
    }

    #[test]
    fn finding_lyrics_again_replaces_none_found_and_lines_them_up_afresh_but_keeps_lyrics_when_nothing_is_found() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let (t, s, lrc) = separated_singing_song(&c, &lib);
        lib.set_lyrics(t, LyricsSource::None, &[], now_ms()).unwrap();
        lib.set_lyric_offset(s, 200).unwrap();
        let mut events = Vec::new();
        assert!(find_lyrics_again(&c, &lib, &Lrc(lrc), t, &mut |e| events.push(e)).unwrap());
        assert_eq!(events, [Event::LyricOffset { track_id: t }, Event::Lyrics { track_id: t }, Event::LyricOffset { track_id: t }]);
        assert_eq!(lib.selected_source(t).unwrap().unwrap().lyric_offset_ms, 1_500);
        assert!(!find_lyrics_again(&c, &lib, &NoLyrics, t, &mut |_| {}).unwrap());
        assert_eq!(lib.lyrics(t).unwrap().unwrap().source, LyricsSource::Lrclib);
    }

    #[test]
    fn a_ready_song_lines_its_lyrics_up_once_per_saved_lyrics_unless_the_user_set_the_timing() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let (t, s, lrc) = separated_singing_song(&c, &lib);
        let save_lyrics = |at: i64| lib.set_lyrics(t, LyricsSource::Lrclib, &lyrics::parse_lrc(&lrc, 120_000), at).unwrap();
        let play = || {
            let (r, events) = run(&c, &lib, t, &AtomicBool::new(false));
            assert_eq!(r.unwrap(), Outcome::Done);
            let synced = events.iter().filter(|e| **e == Event::LyricOffset { track_id: t }).count();
            (lib.selected_source(t).unwrap().unwrap().lyric_offset_ms, synced)
        };
        save_lyrics(1);
        assert_eq!(play(), (1_500, 1));
        lib.set_auto_lyric_offset(s, 0).unwrap();
        assert_eq!(play(), (0, 0));
        save_lyrics(2);
        assert_eq!(play(), (1_500, 1));
        lib.set_lyric_offset(s, 200).unwrap();
        save_lyrics(3);
        assert_eq!(play(), (200, 0));
    }

    #[test]
    fn lyrics_that_arrive_for_a_separated_song_are_lined_up() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let (t, _, lrc) = separated_singing_song(&c, &lib);
        let (lyrics, rx) = lookup(&c, Lrc(lrc));
        lyrics.request(t);
        lyrics.finish();
        assert_eq!(rx.try_iter().collect::<Vec<_>>(), vec![Event::Lyrics { track_id: t }, Event::LyricOffset { track_id: t }]);
        assert_eq!(lib.selected_source(t).unwrap().unwrap().lyric_offset_ms, 1_500);
    }

    #[test]
    fn events_serialize_for_the_app() {
        let json = serde_json::to_value(Event::Progress { track_id: 1, chunks_done: 2, chunks_total: 3 }).unwrap();
        assert_eq!(json, serde_json::json!({ "kind": "progress", "trackId": 1, "chunksDone": 2, "chunksTotal": 3 }));
        let json = serde_json::to_value(Event::Stage { track_id: 1, stage: Stage::FindingLyrics }).unwrap();
        assert_eq!(json["stage"], "findingLyrics");
        let json = serde_json::to_value(Event::Failed { track_id: 1, message: String::new(), problem: Some(Problem::FileMoved) }).unwrap();
        assert_eq!(json["problem"], "fileMoved");
    }

    fn titled(lib: &Library, title: &str) -> i64 {
        lib.add_track(&crate::library::NewTrack { provider: crate::library::ProviderId::Local, provider_ref: None, title, artist: None, album: None, duration_ms: None }).unwrap()
    }

    /// Reports each title it is asked about, then waits for the test to let it finish.
    struct Paced(mpsc::Receiver<()>, mpsc::Sender<String>);
    impl LyricsFetcher for Paced {
        fn fetch(&self, title: &str, _: Option<&str>, _: u64) -> Result<Option<String>> {
            self.1.send(title.to_string())?;
            self.0.recv_timeout(Duration::from_secs(10))?;
            Ok(None)
        }
    }

    #[test]
    fn lyrics_lookups_take_the_newest_request_and_skip_songs_no_longer_wanted() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let (x, a, b, cc) = (titled(&lib, "x"), titled(&lib, "a"), titled(&lib, "b"), titled(&lib, "c"));
        let (step, paced) = mpsc::channel();
        let (asked_tx, asked) = mpsc::channel();
        let (lyrics, _) = lookup(&c, Paced(paced, asked_tx));
        lyrics.request(x);
        assert_eq!(asked.recv_timeout(Duration::from_secs(10)).unwrap(), "x");
        for t in [a, b, cc] {
            lyrics.request(t);
        }
        lyrics.retain(&[a, cc]);
        for _ in 0..3 {
            step.send(()).unwrap();
        }
        lyrics.finish();
        assert_eq!(asked.try_iter().collect::<Vec<_>>(), vec!["c", "a"]);
    }

    #[test]
    fn dropping_the_lookup_drops_its_waiting_requests() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let (x, a) = (titled(&lib, "x"), titled(&lib, "a"));
        let (step, paced) = mpsc::channel();
        let (asked_tx, asked) = mpsc::channel();
        let (lyrics, _) = lookup(&c, Paced(paced, asked_tx));
        lyrics.request(x);
        assert_eq!(asked.recv_timeout(Duration::from_secs(10)).unwrap(), "x");
        lyrics.request(a);
        drop(lyrics);
        drop(step);
        assert_eq!(asked.iter().collect::<Vec<_>>(), Vec::<String>::new());
    }

    #[test]
    fn worker_prepares_the_queue_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let a = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        let b = ingest::add_file(&lib, &song(dir.path(), "b.wav")).unwrap().track_id;
        let (w, rx) = worker(&c, Silence);
        w.play(vec![a, b]);
        let seen = wait_for(&rx, &Event::Ready { track_id: b });
        let pos = |id| seen.iter().position(|e| e == &Event::Ready { track_id: id });
        assert!(pos(a) < pos(b));
    }

    #[test]
    fn asking_again_for_a_ready_song_reports_it_ready_again() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let a = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        let b = ingest::add_file(&lib, &song(dir.path(), "b.wav")).unwrap().track_id;
        let (w, rx) = worker(&c, Silence);
        w.play(vec![a, b]);
        wait_for(&rx, &Event::Ready { track_id: b });
        w.play(vec![a]);
        wait_for(&rx, &Event::Ready { track_id: a });
    }

    #[test]
    fn worker_drops_a_song_when_you_switch_away() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let a = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        let b = ingest::add_file(&lib, &song(dir.path(), "b.wav")).unwrap().track_id;
        let (w, rx) = worker(&c, Slow);
        w.play(vec![a]);
        wait_for(&rx, &Event::Stage { track_id: a, stage: Stage::Separating });
        w.play(vec![b]);
        let seen = wait_for(&rx, &Event::Ready { track_id: b });
        assert!(!seen.contains(&Event::Ready { track_id: a }));
    }

    #[test]
    fn switching_back_before_the_cancel_lands_still_finishes_the_song() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let a = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        let b = ingest::add_file(&lib, &song(dir.path(), "b.wav")).unwrap().track_id;
        let (w, rx) = worker(&c, Slow);
        w.play(vec![a]);
        wait_for(&rx, &Event::Stage { track_id: a, stage: Stage::Separating });
        w.play(vec![b]);
        w.play(vec![a]);
        wait_for(&rx, &Event::Ready { track_id: a });
    }

    #[test]
    fn the_playing_song_is_never_evicted() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        lib.set_setting("cache_budget_bytes", "1").unwrap();
        // Distinct frequencies so a, b and z hash (and get cached) separately.
        let path = |name, freq| {
            let p = dir.path().join(name);
            write_sine_wav(&p, 44_100, 2, 2.5, freq);
            p
        };
        let a = ingest::add_file(&lib, &path("a.wav", 330.0)).unwrap().track_id;
        let b = ingest::add_file(&lib, &path("b.wav", 440.0)).unwrap().track_id;
        let z = ingest::add_file(&lib, &path("z.wav", 550.0)).unwrap().track_id;
        let (w, rx) = worker(&c, Silence);
        w.play(vec![a, b, z]);
        // The budget pass that runs right after b finishes happens, in the
        // worker's own thread, strictly before z starts fetching.
        wait_for(&rx, &Event::Stage { track_id: z, stage: Stage::Fetching });
        let hash = lib.selected_source(a).unwrap().unwrap().audio_hash.unwrap();
        assert_eq!(lib.separation(&hash, "test").unwrap().unwrap().status, SepStatus::Ready);
    }

    /// Runs ffmpeg quietly to build a test fixture at `out` (tests only; the app never uses ffmpeg).
    fn ffmpeg(args: &[&str], out: &Path) {
        let status = std::process::Command::new("ffmpeg")
            .args(["-y", "-loglevel", "error"])
            .args(args)
            .arg(out)
            .status()
            .expect("this test needs ffmpeg to build fixture files");
        assert!(status.success());
    }

    #[test]
    fn cover_art_is_saved_once_and_shared_between_songs() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let cover = dir.path().join("cover.png");
        ffmpeg(&["-f", "lavfi", "-i", "color=c=orange:s=8x8", "-frames:v", "1"], &cover);
        let arts: Vec<String> = [330, 440]
            .iter()
            .map(|freq| {
                let p = dir.path().join(format!("{freq}.mp3"));
                let sine = format!("sine=frequency={freq}:duration=2.5:sample_rate=44100");
                ffmpeg(&["-f", "lavfi", "-i", &sine, "-i", cover.to_str().unwrap(), "-map", "0:a", "-map", "1:v", "-c:v", "png", "-disposition:v", "attached_pic", "-id3v2_version", "3", "-ac", "2"], &p);
                let t = ingest::add_file(&lib, &p).unwrap().track_id;
                run(&c, &lib, t, &AtomicBool::new(false)).0.unwrap();
                lib.track(t).unwrap().artwork_path.unwrap()
            })
            .collect();
        assert_eq!(arts[0], arts[1]);
        assert!(arts[0].ends_with(".png") && Path::new(&arts[0]).exists());
    }
}
