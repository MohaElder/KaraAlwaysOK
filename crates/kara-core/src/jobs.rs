//! Preparing a song for singing: fetch → standardize → (lyrics ‖ separate).
//! `Worker` runs one song at a time: the one you're about to sing first, then
//! the rest of the queue. Switching songs cancels the old one.

use crate::audio::{self, Stereo, SAMPLE_RATE};
use crate::cache;
use crate::ingest;
use crate::library::{AudioSource, Library, LyricsSource, SepStatus, SeparationRow, SourceKind, SourceStatus, Track};
use crate::lyrics::{self, LyricsFetcher};
use crate::now_ms;
use crate::separate::mdx::{self, MdxParams, Outcome, VocalModel};
use crate::store::{write_atomic, Store};
use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread::JoinHandle;

#[derive(Clone, Debug, PartialEq)]
pub enum Stage {
    Fetching,
    Standardizing,
    Lyrics,
    Separating,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Stage { track_id: i64, stage: Stage },
    Progress { track_id: i64, chunks_done: u32, chunks_total: u32 },
    Ready { track_id: i64 },
    Failed { track_id: i64, message: String },
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

/// Prepares one song: fetches and standardizes its audio if needed, then
/// looks up lyrics and separates vocals in parallel. Emits progress on `emit`
/// and marks the source failed with a plain-language message on error.
pub fn prepare(
    ctx: &Ctx,
    lib: &Library,
    model: &mut dyn VocalModel,
    fetcher: &(dyn LyricsFetcher + Sync),
    track_id: i64,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Event),
) -> Result<Outcome> {
    let result = prepare_inner(ctx, lib, model, fetcher, track_id, cancel, emit);
    if let Err(e) = &result {
        // Only the outermost context reaches the user.
        let message = e.to_string();
        if let Ok(Some(src)) = lib.selected_source(track_id) {
            let _ = lib.set_source_status(src.id, SourceStatus::Failed, Some(&message));
        }
        emit(Event::Failed { track_id, message });
    }
    result
}

fn is_ready(ctx: &Ctx, lib: &Library, hash: &str) -> Result<bool> {
    Ok(lib.separation(hash, &ctx.model_id)?.is_some_and(|r| r.status == SepStatus::Ready))
}

fn prepare_inner(
    ctx: &Ctx,
    lib: &Library,
    model: &mut dyn VocalModel,
    fetcher: &(dyn LyricsFetcher + Sync),
    track_id: i64,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Event),
) -> Result<Outcome> {
    let track = lib.track(track_id)?;
    let source = lib.selected_source(track_id)?.context("This song has no audio to play.")?;
    if let Some(hash) = &source.audio_hash {
        if is_ready(ctx, lib, hash)? {
            lib.touch_separation(hash, &ctx.model_id, now_ms())?;
            emit(Event::Ready { track_id });
            return Ok(Outcome::Done);
        }
    }

    let (hash, mix) = load_or_fetch(ctx, lib, &track, &source, emit)?;
    if is_ready(ctx, lib, &hash)? {
        lib.touch_separation(&hash, &ctx.model_id, now_ms())?;
        emit(Event::Ready { track_id });
        return Ok(Outcome::Done);
    }
    if cancel.load(Ordering::Relaxed) {
        return Ok(Outcome::Cancelled);
    }

    let track = lib.track(track_id)?; // fetching may have filled in title/artist
    let need_lyrics = match lib.lyrics(track_id)? {
        None => true,
        Some(l) => l.source == LyricsSource::None && now_ms() - l.fetched_at > LYRICS_RETRY_MS,
    };
    emit(Event::Stage { track_id, stage: Stage::Lyrics });
    std::thread::scope(|s| {
        let lookup = need_lyrics.then(|| {
            let track = &track;
            s.spawn(move || {
                let dur = track.duration_ms.unwrap_or(0);
                lyrics::find(None, &track.title, track.artist.as_deref(), track.album.as_deref(), dur, fetcher)
            })
        });
        let outcome = separate_stage(ctx, lib, track_id, &hash, &mix, model, cancel, emit);
        if let Some(handle) = lookup {
            // Offline or the service is down: leave lyrics unset so we try again next time.
            if let Ok(Ok((src, lines))) = handle.join() {
                lib.set_lyrics(track_id, src, &lines, now_ms())?;
            }
        }
        outcome
    })
}

/// The song in the standard format, plus its hash. Uses the stored copy when
/// there is one; otherwise fetches and converts the original.
fn load_or_fetch(ctx: &Ctx, lib: &Library, track: &Track, source: &AudioSource, emit: &mut dyn FnMut(Event)) -> Result<(String, Stereo)> {
    if let Some(hash) = &source.audio_hash {
        let p = ctx.store.source_path(hash);
        if p.exists() {
            return Ok((hash.clone(), audio::read_flac(&p).context("Couldn't read this song's audio.")?));
        }
    }
    emit(Event::Stage { track_id: track.id, stage: Stage::Fetching });
    lib.set_source_status(source.id, SourceStatus::Fetching, None)?;
    let path = ingest::fetch_audio(lib, &ctx.store, track, source)?;
    emit(Event::Stage { track_id: track.id, stage: Stage::Standardizing });
    let decoded = audio::decode_file(&path).context("Couldn't read this audio. The format may not be supported.")?;
    if source.kind == SourceKind::Link {
        let _ = std::fs::remove_file(&path);
    }
    anyhow::ensure!(!decoded.audio.is_empty(), "This audio is empty.");
    let hash = audio::audio_hash(&decoded.audio);
    let dst = ctx.store.source_path(&hash);
    if !is_ready(ctx, lib, &hash)? && !dst.exists() {
        write_atomic(&dst, &audio::encode_flac(&decoded.audio)?).context("Couldn't save the audio. Is the disk full?")?;
    }
    lib.set_source_audio(source.id, &hash, decoded.audio.duration_ms())?;
    if let Some(text) = decoded.tags.lyrics.as_deref().filter(|t| lyrics::is_synced(t)) {
        let lines = lyrics::parse_lrc(text, decoded.audio.duration_ms());
        lib.set_lyrics(track.id, LyricsSource::Embedded, &lines, now_ms())?;
    }
    Ok((hash, decoded.audio))
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
        size_bytes: 0,
        last_used_at: Some(now_ms()),
    });
    row.chunks_done = start;
    row.status = SepStatus::Running;
    lib.upsert_separation(&row)?;
    emit(Event::Progress { track_id, chunks_done: start, chunks_total: total });

    let outcome = mdx::separate(model, &ctx.params, mix, ctx.chunk_len, start as usize, cancel, |chunk| {
        cache::write_chunk(&ctx.store, hash, &ctx.model_id, &chunk)?;
        row.chunks_done = chunk.index as u32 + 1;
        lib.upsert_separation(&row)?;
        emit(Event::Progress { track_id, chunks_done: row.chunks_done, chunks_total: total });
        Ok(())
    })
    .context("Couldn't take the vocals out of this song.")?;

    match outcome {
        Outcome::Cancelled => {
            row.status = SepStatus::Cancelled;
            lib.upsert_separation(&row)?;
        }
        Outcome::Done => {
            cache::finish(&ctx.store, lib, hash, &ctx.model_id)?;
            emit(Event::Ready { track_id });
        }
    }
    Ok(outcome)
}

// ---------------------------------------------------------------- worker

struct State {
    queue: VecDeque<i64>,
    running: Option<(i64, Arc<AtomicBool>)>,
    shutdown: bool,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

pub struct Worker {
    shared: Arc<Shared>,
    handle: Option<JoinHandle<()>>,
}

impl Worker {
    /// Starts a background thread that prepares queued songs one at a time
    /// and reports their events on `events`.
    pub fn spawn(ctx: Ctx, mut model: Box<dyn VocalModel>, fetcher: Box<dyn LyricsFetcher + Send + Sync>, events: mpsc::Sender<Event>) -> Self {
        let shared = Arc::new(Shared { state: Mutex::new(State { queue: VecDeque::new(), running: None, shutdown: false }), wake: Condvar::new() });
        let s = shared.clone();
        let handle = std::thread::spawn(move || {
            let lib = match Library::open(&ctx.store.db_path()) {
                Ok(l) => l,
                Err(_) => return,
            };
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
                let _ = prepare(&ctx, &lib, model.as_mut(), fetcher.as_ref(), track, &flag, &mut |e| {
                    let _ = events.send(e);
                });
                let upcoming: Vec<i64> = {
                    let mut st = s.state.lock().unwrap();
                    st.running = None;
                    st.queue.iter().copied().collect()
                };
                let protected: Vec<String> = std::iter::once(track)
                    .chain(upcoming)
                    .filter_map(|t| lib.selected_source(t).ok().flatten()?.audio_hash)
                    .collect();
                let _ = cache::enforce_budget(&ctx.store, &lib, &protected);
            }
        });
        Self { shared, handle: Some(handle) }
    }

    /// `tracks[0]` is the song to prepare now; the rest are prepared next, in order.
    /// A running job for any other song is cancelled.
    pub fn play(&self, tracks: Vec<i64>) {
        let mut st = self.shared.state.lock().unwrap();
        let mut queue: VecDeque<i64> = tracks.into();
        if let Some((running, flag)) = &st.running {
            if queue.front() == Some(running) {
                queue.pop_front(); // already being prepared
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
        fn fetch(&self, _: &str, _: &str, _: Option<&str>, _: u64) -> Result<Option<String>> {
            Ok(None)
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

    fn run(c: &Ctx, lib: &Library, track: i64, cancel: &AtomicBool) -> (Result<Outcome>, Vec<Event>) {
        let mut events = Vec::new();
        let r = prepare(c, lib, &mut Silence, &NoLyrics, track, cancel, &mut |e| events.push(e));
        (r, events)
    }

    #[test]
    fn prepares_a_local_file_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let t = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        let (r, events) = run(&c, &lib, t, &AtomicBool::new(false));
        assert_eq!(r.unwrap(), Outcome::Done);
        assert_eq!(events.first(), Some(&Event::Stage { track_id: t, stage: Stage::Fetching }));
        assert!(events.contains(&Event::Progress { track_id: t, chunks_done: 3, chunks_total: 3 }));
        assert_eq!(events.last(), Some(&Event::Ready { track_id: t }));
        let hash = lib.selected_source(t).unwrap().unwrap().audio_hash.unwrap();
        assert_eq!(lib.separation(&hash, "test").unwrap().unwrap().status, SepStatus::Ready);
        assert!(!c.store.source_path(&hash).exists());
        assert_eq!(lib.lyrics(t).unwrap().unwrap().source, LyricsSource::None);
        // Second time: instant.
        let (_, again) = run(&c, &lib, t, &AtomicBool::new(false));
        assert_eq!(again, vec![Event::Ready { track_id: t }]);
    }

    #[test]
    fn prepare_resumes_after_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let t = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        let cancel = AtomicBool::new(false);
        let r = prepare(&c, &lib, &mut Silence, &NoLyrics, t, &cancel, &mut |e| {
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
        let p = song(dir.path(), "a.wav");
        let t1 = ingest::add_file(&lib, &p).unwrap().track_id;
        let t2 = ingest::add_file(&lib, &p).unwrap().track_id;
        run(&c, &lib, t1, &AtomicBool::new(false)).0.unwrap();
        let (_, events) = run(&c, &lib, t2, &AtomicBool::new(false));
        assert!(!events.iter().any(|e| matches!(e, Event::Progress { .. })));
        assert_eq!(events.last(), Some(&Event::Ready { track_id: t2 }));
        let hash = lib.selected_source(t2).unwrap().unwrap().audio_hash.unwrap();
        assert!(!c.store.source_path(&hash).exists());
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
        assert_eq!(events.last(), Some(&Event::Failed { track_id: t, message: "The file was moved or deleted.".into() }));
        let s = lib.selected_source(t).unwrap().unwrap();
        assert_eq!((s.status, s.error.as_deref()), (SourceStatus::Failed, Some("The file was moved or deleted.")));
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
    fn worker_prepares_the_queue_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let a = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        let b = ingest::add_file(&lib, &song(dir.path(), "b.wav")).unwrap().track_id;
        let (tx, rx) = mpsc::channel();
        let w = Worker::spawn(c, Box::new(Silence), Box::new(NoLyrics), tx);
        w.play(vec![a, b]);
        let seen = wait_for(&rx, &Event::Ready { track_id: b });
        let pos = |id| seen.iter().position(|e| e == &Event::Ready { track_id: id });
        assert!(pos(a) < pos(b));
    }

    #[test]
    fn worker_drops_a_song_when_you_switch_away() {
        struct Slow;
        impl VocalModel for Slow {
            fn infer(&mut self, i: ndarray::Array4<f32>) -> Result<ndarray::Array4<f32>> {
                std::thread::sleep(Duration::from_millis(2));
                Ok(ndarray::Array4::zeros(i.dim()))
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let a = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        let b = ingest::add_file(&lib, &song(dir.path(), "b.wav")).unwrap().track_id;
        let (tx, rx) = mpsc::channel();
        let w = Worker::spawn(c, Box::new(Slow), Box::new(NoLyrics), tx);
        w.play(vec![a]);
        wait_for(&rx, &Event::Stage { track_id: a, stage: Stage::Separating });
        w.play(vec![b]);
        let seen = wait_for(&rx, &Event::Ready { track_id: b });
        assert!(!seen.contains(&Event::Ready { track_id: a }));
    }
}
