use crate::library::{cards, CollectionCard};
use crate::state::{coded, AppError, AppState, Plain};
use anyhow::Context;
use kara_core::ingest::link::{self, LinkVerdict};
use kara_core::ingest::preview::{self, LinkPreview};
use kara_core::ingest::ytdlp;
use kara_core::library::{CollectionKind, Library, Track};
use serde::Serialize;
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
    let query = input.trim().to_lowercase();
    if query.is_empty() {
        return Ok(SearchOutcome::Text { tracks: Vec::new(), collections: Vec::new() });
    }
    let mut collections = Vec::new();
    for kind in [CollectionKind::Playlist, CollectionKind::Album, CollectionKind::Artist] {
        collections.extend(cards(lib, kind)?.into_iter().filter(|c| {
            let name = if kind == CollectionKind::Playlist && !c.row.user { imported } else { &c.row.name };
            name.to_lowercase().contains(&query)
        }));
    }
    Ok(SearchOutcome::Text { tracks: lib.search(input, 50)?, collections })
}

#[tauri::command]
pub fn search(state: State<'_, AppState>, input: String, imported: String) -> Result<SearchOutcome, AppError> {
    search_input(&state.lib.lock().unwrap(), &input, &imported).plain()
}

/// Streams what a pasted link points to: the site's quick preview when it has one, then the full details.
#[tauri::command]
pub async fn link_preview(state: State<'_, AppState>, url: String, on_update: Channel<LinkPreview>) -> Result<(), AppError> {
    let bin_dir = state.store.bin_dir();
    tauri::async_runtime::spawn_blocking(move || -> anyhow::Result<()> {
        let url = link::parse_link(&url).context(kara_core::problem::Problem::NotALink)?;
        if link::verdict(&url) == LinkVerdict::AudioFile {
            let _ = on_update.send(preview::file_preview(&url));
            return Ok(());
        }
        let quick = preview::oembed(&url).ok().flatten();
        if let Some(p) = &quick {
            let _ = on_update.send(p.clone());
        }
        match ytdlp::ensure(&bin_dir).and_then(|bin| preview::probe(&bin, &url)) {
            Ok(full) => {
                let _ = on_update.send(full);
                Ok(())
            }
            Err(_) if quick.is_some() => Ok(()),
            Err(e) => Err(coded(e, kara_core::problem::Problem::NoSongAtLink)),
        }
    })
    .await
    .map_err(AppError::from)?
    .plain()
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
        assert_eq!((ids("読み込"), ids("imp")), (vec![Some(imported)], vec![]));
        let link = json("youtu.be/abc");
        assert_eq!((link["kind"].as_str(), link["host"].as_str()), (Some("link"), Some("youtu.be")));
        let refused = json("https://open.spotify.com/track/x");
        assert_eq!((refused["kind"].as_str(), refused["streaming"].as_bool(), refused["host"].as_str()), (Some("rejected"), Some(true), Some("spotify.com")));
    }
}
