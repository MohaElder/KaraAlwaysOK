use crate::library::{cards, CollectionCard};
use crate::state::{coded, AppError, AppState, Plain};
use anyhow::Context;
use kara_core::fuzzy::{best_first, Fuzzy};
use kara_core::ingest::{self, Ingested};
use kara_core::ingest::link::{self, LinkVerdict};
use kara_core::ingest::preview::{self, LinkPreview, SearchHit};
use kara_core::ingest::{bilibili, youtube, ytdlp};
use kara_core::library::{CollectionKind, Library, Track};
use serde::{Deserialize, Serialize};
use std::path::Path;
use tauri::ipc::Channel;
use tauri::State;

#[derive(Serialize)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SearchOutcome {
    Text { tracks: Vec<Track>, collections: Vec<CollectionCard> },
    Link { url: String, host: String },
    Rejected { streaming: bool, host: String },
}

/// What the search bar's text means: a link we can use, a link we can't, or words to look for.
/// The Imported playlist matches by `imported`, the name the user sees for it.
pub fn search_input(lib: &Library, input: &str, imported: &str) -> anyhow::Result<SearchOutcome> {
    if let Some(url) = link::parse_link(input) {
        let host = url.host_str().unwrap_or_default().trim_start_matches("www.").trim_start_matches("open.").to_string();
        return Ok(match link::verdict(&url) {
            LinkVerdict::AudioFile | LinkVerdict::Extractable => SearchOutcome::Link { url: url.into(), host },
            verdict => SearchOutcome::Rejected { streaming: verdict == LinkVerdict::Streaming, host },
        });
    }
    let Some(mut fuzzy) = Fuzzy::new(input) else {
        return Ok(SearchOutcome::Text { tracks: Vec::new(), collections: Vec::new() });
    };
    let mut all = Vec::new();
    for kind in [CollectionKind::Playlist, CollectionKind::Album, CollectionKind::Artist] {
        all.extend(cards(lib, kind)?);
    }
    let collections = best_first(all, |c| fuzzy.score(if c.row.kind == CollectionKind::Playlist && !c.row.user { imported } else { &c.row.name }));
    Ok(SearchOutcome::Text { tracks: lib.search(input, 50)?, collections })
}

#[tauri::command]
pub fn search(state: State<'_, AppState>, input: String, imported: String) -> Result<SearchOutcome, AppError> {
    search_input(&state.lib.lock().unwrap(), &input, &imported).plain()
}

/// What a link points to, passed to `send`: the site's quick preview when it has one, then the full details. Only links
/// that can be added are looked up.
pub fn preview_link(bin_dir: &Path, url: &str, mut send: impl FnMut(LinkPreview)) -> anyhow::Result<()> {
    let url = link::parse_link(url).context(kara_core::problem::Problem::NotALink)?;
    if let Some(problem) = link::rejection(&url) {
        return Err(anyhow::Error::new(problem));
    }
    if link::verdict(&url) == LinkVerdict::AudioFile {
        send(preview::file_preview(&url));
        return Ok(());
    }
    let quick = preview::oembed(&url).ok().flatten();
    if let Some(p) = &quick {
        send(p.clone());
    }
    match ytdlp::ensure(bin_dir).and_then(|bin| preview::probe(&bin, &url)) {
        Ok(full) => {
            send(full);
            Ok(())
        }
        Err(_) if quick.is_some() => Ok(()),
        Err(e) => Err(coded(e, kara_core::problem::Problem::NoSongAtLink)),
    }
}

/// Streams what a pasted link points to: the site's quick preview when it has one, then the full details.
#[tauri::command]
pub async fn link_preview(state: State<'_, AppState>, url: String, on_update: Channel<LinkPreview>) -> Result<(), AppError> {
    let bin_dir = state.store.bin_dir();
    tauri::async_runtime::spawn_blocking(move || preview_link(&bin_dir, &url, |p| drop(on_update.send(p)))).await.map_err(AppError::from)?.plain()
}

/// A site the search bar also finds videos on.
#[derive(Serialize, Deserialize, Clone, Copy)]
#[serde(rename_all = "camelCase")]
pub enum WebSource {
    Youtube,
    Bilibili,
}

/// The top videos on `source` for `query`; none when the site can't be reached. YouTube's web search falls back to yt-dlp's.
pub fn find_on_web(bin_dir: &Path, source: WebSource, query: &str) -> Vec<SearchHit> {
    match source {
        WebSource::Youtube => youtube::search(query).or_else(|_| ytdlp::ensure(bin_dir).and_then(|bin| preview::search(&bin, query))),
        WebSource::Bilibili => bilibili::search(query),
    }
    .unwrap_or_default()
}

/// The top YouTube videos for the search bar's words.
#[tauri::command]
pub async fn youtube_search(state: State<'_, AppState>, query: String) -> Result<Vec<SearchHit>, AppError> {
    let bin_dir = state.store.bin_dir();
    tauri::async_runtime::spawn_blocking(move || find_on_web(&bin_dir, WebSource::Youtube, &query)).await.map_err(AppError::from)
}

/// YouTube's suggestions for the search bar's words; none when YouTube can't be reached.
#[tauri::command]
pub async fn youtube_suggestions(query: String) -> Result<Vec<String>, AppError> {
    tauri::async_runtime::spawn_blocking(move || youtube::suggestions(&query).unwrap_or_default()).await.map_err(AppError::from)
}

/// The top Bilibili videos for the search bar's words.
#[tauri::command]
pub async fn bilibili_search(state: State<'_, AppState>, query: String) -> Result<Vec<SearchHit>, AppError> {
    let bin_dir = state.store.bin_dir();
    tauri::async_runtime::spawn_blocking(move || find_on_web(&bin_dir, WebSource::Bilibili, &query)).await.map_err(AppError::from)
}

/// Bilibili's suggestions for the search bar's words; none when Bilibili can't be reached.
#[tauri::command]
pub async fn bilibili_suggestions(query: String) -> Result<Vec<String>, AppError> {
    tauri::async_runtime::spawn_blocking(move || bilibili::suggestions(&query).unwrap_or_default()).await.map_err(AppError::from)
}

/// Puts a dropped file in Imported; `start_adding` then gets it ready.
#[tauri::command]
pub fn add_file(state: State<'_, AppState>, path: String) -> Result<Track, AppError> {
    let lib = state.lib.lock().unwrap();
    let added = ingest::add_file(&lib, Path::new(&path)).plain()?;
    lib.track(added.track_id).plain()
}

/// Puts a pasted link in Imported; `start_adding` then downloads it.
#[tauri::command]
pub fn add_link(state: State<'_, AppState>, url: String) -> Result<Track, AppError> {
    let lib = state.lib.lock().unwrap();
    let added = ingest_link(&lib, &url)?;
    lib.track(added.track_id).plain()
}

/// Puts a link in Imported, or finds the song already made from it.
pub fn ingest_link(lib: &Library, url: &str) -> Result<Ingested, AppError> {
    let url = link::parse_link(url).context(kara_core::problem::Problem::NotALink).plain()?;
    ingest::add_link(lib, &url).plain()
}

/// Whether a song's audio has been fetched.
pub fn has_audio(lib: &Library, track_id: i64) -> anyhow::Result<bool> {
    Ok(lib.selected_source(track_id)?.is_some_and(|s| s.audio_hash.is_some()))
}

/// Gets a newly added song's audio and lyrics in the background; false when the song
/// already has its audio or is queued (the worker gets it then).
#[tauri::command]
pub fn start_adding(state: State<'_, AppState>, track_id: i64) -> Result<bool, AppError> {
    let queued = state.player.lock().unwrap().upcoming().contains(&track_id);
    let has_audio = has_audio(&state.lib.lock().unwrap(), track_id).plain()?;
    if queued || has_audio {
        return Ok(false);
    }
    state.adder.add(track_id);
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kara_core::library::{NewTrack, ProviderId};

    #[test]
    fn the_search_bar_tells_links_from_words() {
        let lib = Library::open_in_memory().unwrap();
        let t = lib.add_track(&NewTrack { provider: ProviderId::Local, provider_ref: None, title: "Paper Boats", artist: Some("Juniper Row"), album: None, duration_ms: None }).unwrap();
        kara_core::ingest::link_collections(&lib, t, Some("Juniper Row"), None).unwrap();
        let imported = lib.upsert_collection(ProviderId::Local, CollectionKind::Playlist, "imported", "Imported", None).unwrap();
        lib.add_to_collection(imported, t).unwrap();
        let json = |input: &str| serde_json::to_value(search_input(&lib, input, "読み込んだ曲").unwrap()).unwrap();
        let text = json("juni");
        assert_eq!(
            (text["kind"].as_str(), text["tracks"][0]["title"].as_str(), text["collections"][0]["name"].as_str()),
            (Some("text"), Some("Paper Boats"), Some("Juniper Row"))
        );
        let ids = |input: &str| json(input)["collections"].as_array().unwrap().iter().map(|c| c["id"].as_i64()).collect::<Vec<_>>();
        assert_eq!((ids("込ん曲"), ids("imp")), (vec![Some(imported)], vec![]));
        let link = json("youtu.be/abc");
        assert_eq!((link["kind"].as_str(), link["host"].as_str()), (Some("link"), Some("youtu.be")));
        let refused = json("https://open.spotify.com/track/x");
        assert_eq!((refused["kind"].as_str(), refused["streaming"].as_bool(), refused["host"].as_str()), (Some("rejected"), Some(true), Some("spotify.com")));
    }

    #[test]
    fn only_links_that_can_be_added_are_looked_up() {
        use kara_core::problem::{problem, Problem};
        for (url, why) in [("http://127.0.0.1:8080/admin", Problem::LinkUnsupported), ("https://open.spotify.com/track/x", Problem::LinkStreaming)] {
            let e = preview_link(Path::new("/nonexistent"), url, |_| panic!("{url} was looked up")).unwrap_err();
            assert_eq!(problem(&e), Some(why), "{url}");
        }
    }
}
