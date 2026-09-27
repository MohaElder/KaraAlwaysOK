use crate::state::{AppError, AppState, Plain};
use kara_core::cache;
use kara_core::library::{CollectionKind, CollectionRow, Library, LyricsSource, Track};
use kara_core::lyrics::Line;
use serde::Serialize;
use tauri::{AppHandle, State};

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollectionCard {
    #[serde(flatten)]
    pub row: CollectionRow,
    pub count: usize,
    pub covers: Vec<Track>,
}

#[derive(Serialize)]
pub struct CollectionPage {
    pub card: CollectionCard,
    pub tracks: Vec<Track>,
}

#[derive(Serialize)]
pub struct Lyrics {
    pub source: Option<LyricsSource>,
    pub lines: Vec<Line>,
}

/// A collection with its song count and its first four songs for the cover.
fn card(row: CollectionRow, tracks: &[Track]) -> CollectionCard {
    CollectionCard { count: tracks.len(), covers: tracks.iter().take(4).cloned().collect(), row }
}

/// Every collection of `kind`; empty ones show only if the user made them.
pub fn cards(lib: &Library, kind: CollectionKind) -> anyhow::Result<Vec<CollectionCard>> {
    let mut out = Vec::new();
    for row in lib.collections(None, kind)? {
        let tracks = lib.collection_tracks(row.id)?;
        let c = card(row, &tracks);
        if c.count > 0 || c.row.user {
            out.push(c);
        }
    }
    Ok(out)
}

pub fn page(lib: &Library, id: i64) -> anyhow::Result<CollectionPage> {
    let tracks = lib.collection_tracks(id)?;
    Ok(CollectionPage { card: card(lib.collection(id)?, &tracks), tracks })
}

#[tauri::command]
pub fn list_collections(state: State<'_, AppState>, kind: String) -> Result<Vec<CollectionCard>, AppError> {
    let kind: CollectionKind = kind.parse().plain()?;
    cards(&state.lib.lock().unwrap(), kind).plain()
}

#[tauri::command]
pub fn open_collection(state: State<'_, AppState>, id: i64) -> Result<CollectionPage, AppError> {
    page(&state.lib.lock().unwrap(), id).plain()
}

#[tauri::command]
pub fn get_track(state: State<'_, AppState>, track_id: i64) -> Result<Track, AppError> {
    state.lib.lock().unwrap().track(track_id).plain()
}

#[tauri::command]
pub fn track_lyrics(state: State<'_, AppState>, track_id: i64) -> Result<Lyrics, AppError> {
    let row = state.lib.lock().unwrap().lyrics(track_id).plain()?;
    Ok(match row {
        Some(r) => Lyrics { source: Some(r.source), lines: r.lines },
        None => Lyrics { source: None, lines: Vec::new() },
    })
}

/// Deletes a song: it leaves the queue (so the worker stops on it) before it leaves the library and its audio is freed.
#[tauri::command]
pub fn delete_track(app: AppHandle, state: State<'_, AppState>, track_id: i64) -> Result<(), AppError> {
    crate::player::update(&app, state.inner(), |p, _| {
        p.remove_track(track_id);
        Ok(())
    })?;
    cache::delete_track(&state.store, &state.lib.lock().unwrap(), track_id).plain()
}

#[cfg(test)]
mod tests {
    use super::*;
    use kara_core::library::{NewTrack, ProviderId};

    #[test]
    fn cards_count_songs_show_four_covers_and_skip_empty_imported() {
        let lib = Library::open_in_memory().unwrap();
        lib.upsert_collection(ProviderId::Local, CollectionKind::Playlist, "imported", "Imported", None).unwrap();
        lib.create_playlist("Friday Mix").unwrap();
        let album = lib.upsert_collection(ProviderId::Local, CollectionKind::Album, "album:x", "Demos 2026", Some("Juniper Row")).unwrap();
        for title in ["A", "B", "C", "D", "E"] {
            let t = lib.add_track(&NewTrack { provider: ProviderId::Local, provider_ref: None, title, artist: None, album: None, duration_ms: None }).unwrap();
            lib.add_to_collection(album, t).unwrap();
        }
        let names: Vec<_> = cards(&lib, CollectionKind::Playlist).unwrap().into_iter().map(|c| c.row.name).collect();
        assert_eq!(names, vec!["Friday Mix"]);
        let demos = &cards(&lib, CollectionKind::Album).unwrap()[0];
        assert_eq!((demos.count, demos.covers.len()), (5, 4));
        let json = serde_json::to_value(page(&lib, album).unwrap()).unwrap();
        assert_eq!((json["card"]["name"].as_str(), json["card"]["count"].as_u64(), json["tracks"][4]["title"].as_str()), (Some("Demos 2026"), Some(5), Some("E")));
    }
}
