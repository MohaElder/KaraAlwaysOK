use crate::state::{AppError, AppState, Plain};
use anyhow::Context;
use kara_core::cache::{self, ChunkPcm};
use kara_core::library::{Library, SepStatus, Track};
use kara_core::problem::Problem;
use kara_core::separate::{CHUNK_LEN, DEFAULT_MODEL};
use serde::Serialize;
use tauri::ipc::Response;
use tauri::{AppHandle, Emitter, State};

pub struct Entry {
    pub key: u64,
    pub track_id: i64,
}

/// The play queue: songs in order, which one is current, and whether the last one finished.
#[derive(Default)]
pub struct Player {
    entries: Vec<Entry>,
    current: Option<usize>,
    ended: bool,
    next_key: u64,
}

impl Player {
    fn entry(&mut self, track_id: i64) -> Entry {
        self.next_key += 1;
        Entry { key: self.next_key, track_id }
    }

    /// Nothing is playing: the queue is empty or its last song finished.
    pub fn idle(&self) -> bool {
        self.current.is_none() || self.ended
    }

    pub fn current_track(&self) -> Option<i64> {
        self.current.map(|i| self.entries[i].track_id)
    }

    /// Queues a song at the end, or right after the current one; when idle it becomes the current song.
    pub fn add(&mut self, track_id: i64, next: bool) {
        let e = self.entry(track_id);
        if self.idle() {
            self.entries.push(e);
            self.current = Some(self.entries.len() - 1);
            self.ended = false;
        } else {
            self.entries.insert(if next { self.first_upcoming() } else { self.entries.len() }, e);
        }
    }

    /// Replaces the queue with `tracks`, starting at `start`.
    pub fn play(&mut self, tracks: &[i64], start: usize) {
        self.entries.clear();
        for &t in tracks {
            let e = self.entry(t);
            self.entries.push(e);
        }
        self.current = (!self.entries.is_empty()).then(|| start.min(self.entries.len() - 1));
        self.ended = false;
    }

    /// Moves `delta` songs forward or back; false past either end.
    pub fn skip(&mut self, delta: i64) -> bool {
        let Some(c) = self.current else { return false };
        let to = c as i64 + delta;
        if to < 0 || to >= self.entries.len() as i64 {
            return false;
        }
        self.current = Some(to as usize);
        self.ended = false;
        true
    }

    /// The current song finished: play the next one, or stop at the end.
    pub fn finish(&mut self) {
        if !self.skip(1) {
            self.ended = self.current.is_some();
        }
    }

    fn first_upcoming(&self) -> usize {
        self.current.map_or(0, |c| c + 1)
    }

    /// Moves an upcoming song to queue index `to`, never before the current song.
    pub fn move_entry(&mut self, key: u64, to: usize) {
        let Some(from) = self.entries.iter().position(|e| e.key == key) else { return };
        if from < self.first_upcoming() {
            return;
        }
        let e = self.entries.remove(from);
        let to = to.clamp(self.first_upcoming(), self.entries.len());
        self.entries.insert(to, e);
    }

    /// Takes an upcoming song out of the queue.
    pub fn remove(&mut self, key: u64) {
        if let Some(i) = self.entries.iter().position(|e| e.key == key).filter(|&i| i >= self.first_upcoming()) {
            self.entries.remove(i);
        }
    }

    /// Drops every entry of a deleted song; if it was playing, the next song takes its place.
    #[cfg_attr(not(test), expect(dead_code, reason = "called when a song is deleted"))]
    pub fn remove_track(&mut self, track_id: i64) {
        let Some(c) = self.current else {
            self.entries.retain(|e| e.track_id != track_id);
            return;
        };
        let before = self.entries[..c].iter().filter(|e| e.track_id == track_id).count();
        self.entries.retain(|e| e.track_id != track_id);
        let c = c - before;
        self.current = if self.entries.is_empty() { None } else { Some(c.min(self.entries.len() - 1)) };
        self.ended = self.current.is_some() && (self.ended || c >= self.entries.len());
    }

    /// The current song and the ones after it: what the worker prepares, in order.
    pub fn upcoming(&self) -> Vec<i64> {
        match self.current {
            Some(c) if !self.ended => self.entries[c..].iter().map(|e| e.track_id).collect(),
            _ => Vec::new(),
        }
    }
}

#[derive(Clone, Serialize)]
pub struct QueueEntry {
    pub key: u64,
    pub track: Track,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlayerSnapshot {
    pub entries: Vec<QueueEntry>,
    pub current: Option<usize>,
    pub ended: bool,
    pub lyric_offset_ms: i64,
}

pub fn snapshot(lib: &Library, p: &Player) -> anyhow::Result<PlayerSnapshot> {
    let entries = p.entries.iter().map(|e| Ok(QueueEntry { key: e.key, track: lib.track(e.track_id)? })).collect::<anyhow::Result<_>>()?;
    let lyric_offset_ms = match p.current_track() {
        Some(t) => lib.selected_source(t)?.map_or(0, |s| s.lyric_offset_ms),
        None => 0,
    };
    Ok(PlayerSnapshot { entries, current: p.current, ended: p.ended, lyric_offset_ms })
}

/// Applies `change`, points the worker at the songs now coming up, and broadcasts the new snapshot.
pub(crate) fn update(
    app: &AppHandle,
    state: &AppState,
    change: impl FnOnce(&mut Player, &Library) -> anyhow::Result<()>,
) -> Result<PlayerSnapshot, AppError> {
    let mut p = state.player.lock().unwrap();
    let lib = state.lib.lock().unwrap();
    change(&mut p, &lib).plain()?;
    if let Some(w) = state.worker.lock().unwrap().as_ref() {
        w.play(p.upcoming());
    }
    let snap = snapshot(&lib, &p).plain()?;
    let _ = app.emit("player", &snap);
    Ok(snap)
}

/// Queues a song, refusing one that is no longer in the library.
fn queue_song(p: &mut Player, lib: &Library, track_id: i64, next: bool) -> anyhow::Result<()> {
    lib.track(track_id).context(Problem::SongGone)?;
    p.add(track_id, next);
    Ok(())
}

/// Replaces the queue with `track_ids`, refusing them all if any is no longer in the library.
fn play_songs(p: &mut Player, lib: &Library, track_ids: &[i64], start: usize) -> anyhow::Result<()> {
    for &t in track_ids {
        lib.track(t).context(Problem::SongGone)?;
    }
    p.play(track_ids, start);
    Ok(())
}

fn playing(p: &Player, lib: &Library) -> anyhow::Result<Track> {
    lib.track(p.current_track().context(Problem::NothingPlaying)?)
}

#[tauri::command]
pub fn player_state(state: State<'_, AppState>) -> Result<PlayerSnapshot, AppError> {
    let p = state.player.lock().unwrap();
    snapshot(&state.lib.lock().unwrap(), &p).plain()
}

#[tauri::command]
pub fn queue_add(app: AppHandle, state: State<'_, AppState>, track_id: i64, next: bool) -> Result<PlayerSnapshot, AppError> {
    update(&app, state.inner(), |p, lib| queue_song(p, lib, track_id, next))
}

#[tauri::command]
pub fn play_tracks(app: AppHandle, state: State<'_, AppState>, track_ids: Vec<i64>, start: usize) -> Result<PlayerSnapshot, AppError> {
    update(&app, state.inner(), |p, lib| play_songs(p, lib, &track_ids, start))
}

#[tauri::command]
pub fn skip(app: AppHandle, state: State<'_, AppState>, delta: i64) -> Result<PlayerSnapshot, AppError> {
    update(&app, state.inner(), |p, _| {
        p.skip(delta);
        Ok(())
    })
}

#[tauri::command]
pub fn song_ended(app: AppHandle, state: State<'_, AppState>) -> Result<PlayerSnapshot, AppError> {
    update(&app, state.inner(), |p, _| {
        p.finish();
        Ok(())
    })
}

#[tauri::command]
pub fn queue_move(app: AppHandle, state: State<'_, AppState>, key: u64, to: usize) -> Result<PlayerSnapshot, AppError> {
    update(&app, state.inner(), |p, _| {
        p.move_entry(key, to);
        Ok(())
    })
}

#[tauri::command]
pub fn queue_remove(app: AppHandle, state: State<'_, AppState>, key: u64) -> Result<PlayerSnapshot, AppError> {
    update(&app, state.inner(), |p, _| {
        p.remove(key);
        Ok(())
    })
}

/// The singer slider: 0 keeps the original singer, 100 removes them.
#[tauri::command]
pub fn set_singer(app: AppHandle, state: State<'_, AppState>, value: u8) -> Result<PlayerSnapshot, AppError> {
    update(&app, state.inner(), |p, lib| {
        let t = playing(p, lib)?;
        lib.set_track_settings(t.id, value.min(100), t.key_semitones)
    })
}

#[tauri::command]
pub fn set_key(app: AppHandle, state: State<'_, AppState>, semitones: i8) -> Result<PlayerSnapshot, AppError> {
    update(&app, state.inner(), |p, lib| {
        let t = playing(p, lib)?;
        lib.set_track_settings(t.id, t.vocal_removal, semitones.clamp(-6, 6))
    })
}

#[tauri::command]
pub fn set_lyric_offset(app: AppHandle, state: State<'_, AppState>, ms: i64) -> Result<PlayerSnapshot, AppError> {
    update(&app, state.inner(), |p, lib| {
        let t = playing(p, lib)?;
        let src = lib.selected_source(t.id)?.context(Problem::NoAudio)?;
        lib.set_lyric_offset(src.id, ms.clamp(-5000, 5000))
    })
}

/// Asks the worker to try the current song again after it failed.
#[tauri::command]
pub fn retry_prepare(app: AppHandle, state: State<'_, AppState>) -> Result<(), AppError> {
    update(&app, state.inner(), |_, _| Ok(())).map(|_| ())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaybackInfo {
    pub chunk_frames: u32,
    pub chunks_total: Option<u32>,
    pub chunks_done: u32,
    pub duration_ms: Option<i64>,
}

/// How much of a song's separated audio exists, for the streamer.
pub fn playback(lib: &Library, track_id: i64) -> anyhow::Result<PlaybackInfo> {
    let track = lib.track(track_id)?;
    let row = match lib.selected_source(track_id)?.and_then(|s| s.audio_hash) {
        Some(hash) => lib.separation(&hash, DEFAULT_MODEL.id)?,
        None => None,
    };
    Ok(PlaybackInfo {
        chunk_frames: CHUNK_LEN as u32,
        chunks_total: row.as_ref().map(|r| r.chunks_total),
        chunks_done: row.map_or(0, |r| if r.status == SepStatus::Ready { r.chunks_total } else { r.chunks_done }),
        duration_ms: track.duration_ms,
    })
}

/// A chunk's vocals, then its instrumental, each interleaved stereo f32 little-endian and of equal length.
pub fn pcm_bytes(pcm: &ChunkPcm) -> Vec<u8> {
    let n = pcm.vocals.len().min(pcm.inst.len());
    let mut bytes = Vec::with_capacity(8 * n);
    bytes.extend(pcm.vocals[..n].iter().chain(&pcm.inst[..n]).flat_map(|x| x.to_le_bytes()));
    bytes
}

#[tauri::command]
pub fn playback_info(state: State<'_, AppState>, track_id: i64) -> Result<PlaybackInfo, AppError> {
    playback(&state.lib.lock().unwrap(), track_id).plain()
}

#[tauri::command]
pub async fn chunk_pcm(state: State<'_, AppState>, track_id: i64, index: u32) -> Result<Response, AppError> {
    let (store, reader) = (state.store.clone(), state.reader.clone());
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<Response> {
        let pcm = cache::track_chunk_pcm(&store, &reader.lock().unwrap(), DEFAULT_MODEL.id, track_id, index)?;
        Ok(Response::new(pcm_bytes(&pcm)))
    })
    .await
    .map_err(AppError::from)?
    .plain()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kara_core::library::{NewTrack, ProviderId, SourceKind};

    #[test]
    fn a_song_added_when_idle_starts_and_later_ones_line_up() {
        let mut p = Player::default();
        p.add(1, false);
        assert_eq!((p.current_track(), p.idle()), (Some(1), false));
        p.add(2, false);
        p.add(3, true);
        assert_eq!(p.upcoming(), vec![1, 3, 2]);
        p.finish();
        p.finish();
        p.finish();
        assert!(p.idle() && p.upcoming().is_empty());
        p.add(4, false);
        assert_eq!((p.current_track(), p.upcoming()), (Some(4), vec![4]));
    }

    #[test]
    fn moving_and_removing_only_touch_songs_after_the_current_one() {
        let mut p = Player::default();
        p.play(&[1, 2, 3, 4], 1);
        let key = |p: &Player, t: i64| p.entries.iter().find(|e| e.track_id == t).unwrap().key;
        p.move_entry(key(&p, 4), 2);
        assert_eq!(p.upcoming(), vec![2, 4, 3]);
        p.move_entry(key(&p, 1), 3);
        p.remove(key(&p, 2));
        p.remove(key(&p, 3));
        assert_eq!(p.upcoming(), vec![2, 4]);
        assert!(p.skip(-1));
        assert_eq!(p.current_track(), Some(1));
        assert!(!p.skip(-1) && !p.skip(3));
        assert_eq!(p.current_track(), Some(1));
    }

    #[test]
    fn deleting_the_playing_song_moves_on_to_the_next() {
        let mut p = Player::default();
        p.play(&[1, 2, 1, 3], 1);
        p.remove_track(2);
        assert_eq!(p.upcoming(), vec![1, 3]);
        p.remove_track(1);
        assert_eq!(p.upcoming(), vec![3]);
        p.remove_track(3);
        assert!(p.idle() && p.entries.is_empty());
    }

    #[test]
    fn a_missing_song_is_refused_and_the_queue_stays_as_it_was() {
        let lib = Library::open_in_memory().unwrap();
        let a = lib.add_track(&NewTrack { provider: ProviderId::Local, provider_ref: None, title: "Paper Boats", artist: None, album: None, duration_ms: None }).unwrap();
        let mut p = Player::default();
        queue_song(&mut p, &lib, a, false).unwrap();
        let gone = |r: anyhow::Result<()>| kara_core::problem::problem(&r.unwrap_err());
        assert_eq!(gone(queue_song(&mut p, &lib, 999, true)), Some(Problem::SongGone));
        assert_eq!(gone(play_songs(&mut p, &lib, &[a, 999], 0)), Some(Problem::SongGone));
        assert_eq!((p.upcoming(), snapshot(&lib, &p).unwrap().entries.len()), (vec![a], 1));
    }

    #[test]
    fn the_snapshot_carries_the_queue_and_the_current_lyrics_timing() {
        let lib = Library::open_in_memory().unwrap();
        let add = |title| lib.add_track(&NewTrack { provider: ProviderId::Local, provider_ref: None, title, artist: None, album: None, duration_ms: None }).unwrap();
        let (a, b) = (add("Paper Boats"), add("Rooftop Static"));
        let src = lib.add_source(a, SourceKind::File, "/made/up.wav", None).unwrap();
        lib.set_lyric_offset(src, -300).unwrap();
        let mut p = Player::default();
        p.play(&[a, b], 0);
        let json = serde_json::to_value(snapshot(&lib, &p).unwrap()).unwrap();
        assert_eq!((json["entries"][1]["track"]["title"].as_str(), json["current"].as_u64(), json["lyricOffsetMs"].as_i64()), (Some("Rooftop Static"), Some(0), Some(-300)));
        assert_eq!(json["entries"][0]["track"]["vocalRemoval"], 100);
    }

    #[test]
    fn chunk_bytes_hold_vocals_then_instrumental_in_little_endian() {
        let floats = |pcm| pcm_bytes(&pcm).chunks(4).map(|b| f32::from_le_bytes(b.try_into().unwrap())).collect::<Vec<f32>>();
        assert_eq!(floats(kara_core::cache::ChunkPcm { vocals: vec![0.5, -0.5], inst: vec![0.25, 1.0] }), vec![0.5, -0.5, 0.25, 1.0]);
        assert_eq!(floats(kara_core::cache::ChunkPcm { vocals: vec![0.5, -0.5, 0.1, 0.2], inst: vec![0.25, 1.0] }), vec![0.5, -0.5, 0.25, 1.0]);
    }

    #[test]
    fn playback_info_counts_ready_parts_and_whole_songs() {
        use kara_core::library::{SepStatus, SeparationRow};
        let lib = Library::open_in_memory().unwrap();
        let t = lib.add_track(&NewTrack { provider: ProviderId::Local, provider_ref: None, title: "A", artist: None, album: None, duration_ms: Some(42_000) }).unwrap();
        assert_eq!(playback(&lib, t).unwrap().chunks_total, None);
        let src = lib.add_source(t, SourceKind::File, "/made/up.wav", None).unwrap();
        lib.set_source_audio(src, "h", 42_000).unwrap();
        let mut row = SeparationRow { audio_hash: "h".into(), model_id: kara_core::separate::DEFAULT_MODEL.id.into(), chunk_ms: 10_000, chunks_total: 5, chunks_done: 2, status: SepStatus::Running, last_used_at: None };
        lib.upsert_separation(&row).unwrap();
        let info = playback(&lib, t).unwrap();
        assert_eq!((info.chunks_total, info.chunks_done, info.chunk_frames), (Some(5), 2, 441_000));
        row.status = SepStatus::Ready;
        lib.upsert_separation(&row).unwrap();
        assert_eq!(playback(&lib, t).unwrap().chunks_done, 5);
    }
}
