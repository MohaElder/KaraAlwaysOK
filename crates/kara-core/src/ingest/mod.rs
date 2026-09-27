//! Adding songs (one file or one link) and getting their audio onto disk.

pub mod link;
pub mod ytdlp;

use crate::audio;
use crate::library::{AudioSource, CollectionKind, Library, NewTrack, ProviderId, SourceKind, Track};
use crate::store::{write_atomic, Store};
use anyhow::{bail, Context, Result};
use link::LinkVerdict;
use std::path::{Path, PathBuf};
use url::Url;

pub struct Ingested {
    pub track_id: i64,
    pub source_id: i64,
}

/// "salt-and_static.flac" -> "salt and static"
pub fn title_from_file_name(name: &str) -> String {
    let stem = Path::new(name).file_stem().and_then(|s| s.to_str()).unwrap_or(name);
    stem.split(['-', '_']).filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ").trim().to_string()
}

/// Puts a local track in Local › Imported, plus its album and artist collections.
pub fn link_collections(lib: &Library, track_id: i64, artist: Option<&str>, album: Option<&str>) -> Result<()> {
    let imported = lib.upsert_collection(ProviderId::Local, CollectionKind::Playlist, "imported", "Imported", None)?;
    lib.add_to_collection(imported, track_id)?;
    if let Some(artist) = artist {
        let key = format!("artist:{}", artist.to_lowercase());
        let id = lib.upsert_collection(ProviderId::Local, CollectionKind::Artist, &key, artist, None)?;
        lib.add_to_collection(id, track_id)?;
    }
    if let Some(album) = album {
        let key = format!("album:{}:{}", artist.unwrap_or("").to_lowercase(), album.to_lowercase());
        let id = lib.upsert_collection(ProviderId::Local, CollectionKind::Album, &key, album, artist)?;
        lib.add_to_collection(id, track_id)?;
    }
    Ok(())
}

/// Adds a local file; a file that is already in the library returns its existing track.
pub fn add_file(lib: &Library, path: &Path) -> Result<Ingested> {
    let path = &path.canonicalize().context("The file was moved or deleted.")?;
    let uri = path.display().to_string();
    if let Some(s) = lib.source_by_uri(SourceKind::File, &uri)? {
        return Ok(Ingested { track_id: s.track_id, source_id: s.id });
    }
    let (tags, duration_ms) = audio::read_tags(path).map_err(|e| audio::describe_read_failure(e, "This file isn't audio we can play."))?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("Untitled");
    let title = tags.title.clone().unwrap_or_else(|| title_from_file_name(name));
    let track_id = lib.add_track(&NewTrack {
        provider: ProviderId::Local,
        provider_ref: None,
        title: &title,
        artist: tags.artist.as_deref(),
        album: tags.album.as_deref(),
        duration_ms,
    })?;
    let source_id = lib.add_source(track_id, SourceKind::File, &uri, Some(name))?;
    link_collections(lib, track_id, tags.artist.as_deref(), tags.album.as_deref())?;
    Ok(Ingested { track_id, source_id })
}

/// Adds a link as a track right away; its real title arrives when the audio is fetched.
pub fn add_link(lib: &Library, url: &Url) -> Result<Ingested> {
    if let Some(msg) = link::rejection_message(url) {
        bail!(msg);
    }
    let host = url.host_str().unwrap_or("link").trim_start_matches("www.");
    let title = match link::verdict(url) {
        LinkVerdict::AudioFile => title_from_file_name(url.path_segments().and_then(|mut s| s.next_back()).unwrap_or(host)),
        _ => format!("{host} link"),
    };
    let track_id = lib.add_track(&NewTrack { provider: ProviderId::Local, provider_ref: None, title: &title, artist: None, album: None, duration_ms: None })?;
    let source_id = lib.add_source(track_id, SourceKind::Link, url.as_str(), Some(host))?;
    link_collections(lib, track_id, None, None)?;
    Ok(Ingested { track_id, source_id })
}

fn download_file(url: &Url, dir: &Path) -> Result<PathBuf> {
    let name = url.path_segments().and_then(|mut s| s.next_back()).filter(|s| !s.is_empty()).unwrap_or("download");
    let bytes = reqwest::blocking::get(url.as_str())?.error_for_status()?.bytes()?;
    let path = dir.join(name);
    write_atomic(&path, &bytes)?;
    Ok(path)
}

/// Gets the source's original audio onto disk and returns its path. Links are
/// downloaded into `store.tmp_dir()` (the caller deletes them after decoding).
/// Error messages are shown to the user as-is.
pub fn fetch_audio(lib: &Library, store: &Store, track: &Track, source: &AudioSource) -> Result<PathBuf> {
    match source.kind {
        SourceKind::File => {
            let p = PathBuf::from(&source.uri);
            if !p.exists() {
                bail!("The file was moved or deleted.");
            }
            Ok(p)
        }
        SourceKind::Link => {
            let url = Url::parse(&source.uri)?;
            match link::verdict(&url) {
                LinkVerdict::AudioFile => download_file(&url, &store.tmp_dir()).context("Couldn't download this song. Check the link and your connection."),
                LinkVerdict::Extractable => {
                    let bin = ytdlp::ensure(&store.bin_dir()).context("Couldn't set up downloading. Check your connection.")?;
                    let f = ytdlp::download(&bin, url.as_str(), &store.tmp_dir())
                        .context("Couldn't download this song. Check the link and your connection.")?;
                    lib.update_track_meta(track.id, &f.title, f.artist.as_deref(), f.album.as_deref())?;
                    link_collections(lib, track.id, f.artist.as_deref(), f.album.as_deref())?;
                    Ok(f.path)
                }
                _ => bail!(link::rejection_message(&url).unwrap_or_default()),
            }
        }
        SourceKind::Match => bail!("Songs from streaming libraries arrive in a later version."),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{CollectionKind, ProviderId, SourceStatus};
    use crate::test_util::write_sine_wav;

    #[test]
    fn file_names_become_titles() {
        assert_eq!(title_from_file_name("salt-and_static.flac"), "salt and static");
        assert_eq!(title_from_file_name("noext"), "noext");
    }

    #[test]
    fn add_file_creates_track_source_and_imported_playlist() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("rooftop-static.wav");
        write_sine_wav(&p, 44_100, 2, 1.0, 440.0);
        let lib = Library::open_in_memory().unwrap();
        let ing = add_file(&lib, &p).unwrap();
        let t = lib.track(ing.track_id).unwrap();
        assert_eq!((t.title.as_str(), t.provider, t.duration_ms), ("rooftop static", ProviderId::Local, Some(1000)));
        let src = lib.selected_source(ing.track_id).unwrap().unwrap();
        assert_eq!((src.kind, src.status), (SourceKind::File, SourceStatus::Pending));
        let pls = lib.collections(Some(ProviderId::Local), CollectionKind::Playlist).unwrap();
        assert_eq!(pls[0].name, "Imported");
        assert_eq!(lib.collection_tracks(pls[0].id).unwrap()[0].id, ing.track_id);
    }

    #[test]
    fn re_adding_the_same_file_returns_the_existing_track() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        let p = dir.path().join("a.wav");
        write_sine_wav(&p, 44_100, 2, 0.2, 440.0);
        let lib = Library::open_in_memory().unwrap();
        let first = add_file(&lib, &p).unwrap();
        let again = add_file(&lib, &dir.path().join("sub/../a.wav")).unwrap();
        assert_eq!((again.track_id, again.source_id), (first.track_id, first.source_id));
        let src = lib.selected_source(first.track_id).unwrap().unwrap();
        assert_eq!(Path::new(&src.uri), p.canonicalize().unwrap());
        let imported = &lib.collections(None, CollectionKind::Playlist).unwrap()[0];
        assert_eq!(lib.collection_tracks(imported.id).unwrap().len(), 1);
    }

    #[test]
    fn link_collections_adds_album_and_artist_once() {
        let lib = Library::open_in_memory().unwrap();
        let a = lib.add_track(&crate::library::NewTrack { provider: ProviderId::Local, provider_ref: None, title: "A", artist: None, album: None, duration_ms: None }).unwrap();
        link_collections(&lib, a, Some("Juniper Row"), Some("Demos 2026")).unwrap();
        link_collections(&lib, a, Some("Juniper Row"), Some("Demos 2026")).unwrap(); // second time is a no-op
        assert_eq!(lib.collections(None, CollectionKind::Artist).unwrap().len(), 1);
        let albums = lib.collections(None, CollectionKind::Album).unwrap();
        assert_eq!((albums.len(), albums[0].subtitle.as_deref()), (1, Some("Juniper Row")));
    }

    #[test]
    fn add_link_refuses_streaming_links() {
        let lib = Library::open_in_memory().unwrap();
        let url = link::parse_link("https://open.spotify.com/track/x").unwrap();
        assert!(add_link(&lib, &url).is_err());
        let url = link::parse_link("https://youtu.be/abc").unwrap();
        let ing = add_link(&lib, &url).unwrap();
        assert_eq!(lib.track(ing.track_id).unwrap().title, "youtu.be link");
    }

    #[cfg(unix)]
    #[test]
    fn adding_a_file_we_are_not_allowed_to_read_says_so() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a.wav");
        write_sine_wav(&p, 44_100, 2, 0.2, 440.0);
        std::fs::set_permissions(&p, std::fs::Permissions::from_mode(0o000)).unwrap();
        let lib = Library::open_in_memory().unwrap();
        let err = add_file(&lib, &p).err().unwrap();
        assert_eq!(err.to_string(), "KaraAlwaysOK isn't allowed to read this file.");
    }

    #[test]
    fn fetching_a_moved_file_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("gone.wav");
        write_sine_wav(&p, 44_100, 2, 0.2, 440.0);
        let lib = Library::open_in_memory().unwrap();
        let ing = add_file(&lib, &p).unwrap();
        std::fs::remove_file(&p).unwrap();
        let t = lib.track(ing.track_id).unwrap();
        let s = lib.selected_source(ing.track_id).unwrap().unwrap();
        let err = fetch_audio(&lib, &Store::new(dir.path()), &t, &s).unwrap_err();
        assert_eq!(err.to_string(), "The file was moved or deleted.");
    }
}
