//! Adding songs (one file or one link) and getting their audio onto disk.

pub mod link;
pub mod preview;
pub mod youtube;
pub mod ytdlp;

use crate::audio;
use crate::library::{AudioSource, CollectionKind, Library, NewTrack, ProviderId, SourceKind, Track};
use crate::problem::Problem;
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

const INSTRUMENTAL_WORDS: &[&str] = &["instrumental", "karaoke", "offvocal", "伴奏", "カラオケ"];

/// Whether a song's title, album or tags say it is already instrumental.
pub fn looks_instrumental(texts: &[&str]) -> bool {
    texts.iter().any(|t| {
        let t = t.to_lowercase();
        let squashed: String = t.chars().filter(|c| !matches!(c, ' ' | '-' | '_')).collect();
        INSTRUMENTAL_WORDS.iter().any(|w| squashed.contains(w)) || t.split(|c: char| !c.is_alphanumeric()).any(|w| w == "inst")
    })
}

/// Puts a local track in Local › Imported (newest first), plus its album and artist collections.
pub fn link_collections(lib: &Library, track_id: i64, artist: Option<&str>, album: Option<&str>) -> Result<()> {
    let imported = lib.upsert_collection(ProviderId::Local, CollectionKind::Playlist, "imported", "Imported", None)?;
    lib.add_to_front(imported, track_id)?;
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

/// Saves a local song's title, artist and album, and files it under its new album and artist.
pub fn edit_info(lib: &Library, track_id: i64, title: &str, artist: Option<&str>, album: Option<&str>) -> Result<()> {
    lib.update_track_meta(track_id, title, artist, album)?;
    lib.leave_collections(track_id, CollectionKind::Album)?;
    lib.leave_collections(track_id, CollectionKind::Artist)?;
    link_collections(lib, track_id, artist, album)?;
    lib.prune_empty_collections()
}

/// The file extension to store an image under, from a media type or a file extension.
pub fn image_ext(kind: &str) -> Option<&'static str> {
    match kind.to_ascii_lowercase().trim_start_matches("image/") {
        "jpeg" | "jpg" => Some("jpg"),
        "png" => Some("png"),
        "webp" => Some("webp"),
        _ => None,
    }
}

/// Stores cover art once per distinct image and points the song at it.
pub fn save_artwork(lib: &Library, store: &Store, track_id: i64, bytes: &[u8], ext: &str) -> Result<()> {
    let path = store.artwork_dir().join(format!("{}.{ext}", crate::assets::sha256_hex(bytes)));
    if !path.exists() {
        write_atomic(&path, bytes)?;
    }
    lib.set_artwork(track_id, &path.display().to_string())
}

/// Downloads a thumbnail and saves it, unless the response isn't actually an image.
fn download_artwork(lib: &Library, store: &Store, track_id: i64, url: &str) -> Result<()> {
    let resp = reqwest::blocking::get(url)?.error_for_status()?;
    let ext = resp
        .headers()
        .get(reqwest::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .and_then(|v| image_ext(v.split(';').next().unwrap_or(v)))
        .context("the linked thumbnail isn't an image")?;
    let bytes = resp.bytes()?;
    save_artwork(lib, store, track_id, &bytes, ext)
}

/// Adds a local file; a file that is already in the library returns its existing track.
pub fn add_file(lib: &Library, path: &Path) -> Result<Ingested> {
    let path = &path.canonicalize().map_err(|e| audio::describe_read_failure(e.into(), Problem::NotAudio))?;
    let uri = path.display().to_string();
    if let Some(s) = lib.source_by_uri(SourceKind::File, &uri)? {
        return Ok(Ingested { track_id: s.track_id, source_id: s.id });
    }
    let (tags, duration_ms) = audio::read_tags(path).map_err(|e| audio::describe_read_failure(e, Problem::NotAudio))?;
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
    if looks_instrumental(&[title.as_str(), tags.album.as_deref().unwrap_or_default()]) {
        lib.mark_instrumental(track_id)?;
    }
    let source_id = lib.add_source(track_id, SourceKind::File, &uri, Some(name))?;
    link_collections(lib, track_id, tags.artist.as_deref(), tags.album.as_deref())?;
    Ok(Ingested { track_id, source_id })
}

/// Adds a link as a track right away (a link already in the library returns its existing track); its real title arrives when the audio is fetched.
pub fn add_link(lib: &Library, url: &Url) -> Result<Ingested> {
    let url = &link::canonical(url);
    if let Some(p) = link::rejection(url) {
        bail!(p);
    }
    if let Some(s) = lib.source_by_uri(SourceKind::Link, url.as_str())? {
        return Ok(Ingested { track_id: s.track_id, source_id: s.id });
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
        SourceKind::File => Ok(PathBuf::from(&source.uri)),
        SourceKind::Link => {
            let url = Url::parse(&source.uri)?;
            match link::verdict(&url) {
                LinkVerdict::AudioFile => download_file(&url, &store.tmp_dir()).context(Problem::Download),
                LinkVerdict::Extractable => {
                    let bin_dir = store.bin_dir();
                    let bin = ytdlp::ensure(&bin_dir).context(Problem::DownloaderSetup)?;
                    let f = ytdlp::download(&bin, url.as_str(), &store.tmp_dir(), || ytdlp::update(&bin_dir))
                        .context(Problem::Download)?;
                    lib.update_track_meta(track.id, &f.title, f.artist.as_deref(), f.album.as_deref())?;
                    let mut texts = vec![f.title.as_str(), f.album.as_deref().unwrap_or_default()];
                    texts.extend(f.tags.iter().map(String::as_str));
                    if looks_instrumental(&texts) {
                        lib.mark_instrumental(track.id)?;
                    }
                    link_collections(lib, track.id, f.artist.as_deref(), f.album.as_deref())?;
                    if let Some(url) = f.thumbnail.as_deref().filter(|_| track.artwork_path.is_none()) {
                        let _ = download_artwork(lib, store, track.id, url);
                    }
                    Ok(f.path)
                }
                _ => bail!(link::rejection(&url).unwrap_or(Problem::LinkUnsupported)),
            }
        }
        SourceKind::Match => bail!(Problem::StreamingLater),
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
    fn re_adding_the_same_link_returns_the_existing_track() {
        let lib = Library::open_in_memory().unwrap();
        let url = Url::parse("https://www.youtube.com/watch?v=abc").unwrap();
        let first = add_link(&lib, &url).unwrap();
        let again = add_link(&lib, &url).unwrap();
        assert_eq!((again.track_id, again.source_id), (first.track_id, first.source_id));
        let imported = &lib.collections(None, CollectionKind::Playlist).unwrap()[0];
        assert_eq!(lib.collection_tracks(imported.id).unwrap().len(), 1);
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
    fn editing_info_moves_the_song_to_its_new_album_and_artist() {
        let lib = Library::open_in_memory().unwrap();
        let a = lib.add_track(&crate::library::NewTrack { provider: ProviderId::Local, provider_ref: None, title: "youtu.be link", artist: None, album: None, duration_ms: None }).unwrap();
        link_collections(&lib, a, Some("Made Up Channel"), None).unwrap();
        edit_info(&lib, a, "Paper Boats", Some("Juniper Row"), Some("Demos 2026")).unwrap();
        assert_eq!(lib.track(a).unwrap().title, "Paper Boats");
        let artists: Vec<_> = lib.collections(None, CollectionKind::Artist).unwrap().into_iter().map(|c| c.name).collect();
        assert_eq!(artists, vec!["Juniper Row"]);
        assert_eq!(lib.collections(None, CollectionKind::Album).unwrap()[0].name, "Demos 2026");
        assert_eq!(lib.collection_tracks(lib.collections(None, CollectionKind::Playlist).unwrap()[0].id).unwrap().len(), 1);
    }

    #[test]
    fn add_link_refuses_streaming_links() {
        let lib = Library::open_in_memory().unwrap();
        let url = link::parse_link("https://open.spotify.com/track/x").unwrap();
        assert!(add_link(&lib, &url).is_err());
        let url = link::parse_link("https://youtu.be/abc").unwrap();
        let ing = add_link(&lib, &url).unwrap();
        assert_eq!(lib.track(ing.track_id).unwrap().title, "youtube.com link");
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

    #[cfg(unix)]
    #[test]
    fn adding_a_file_behind_an_unsearchable_folder_says_so() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let sub = dir.path().join("locked");
        std::fs::create_dir(&sub).unwrap();
        let p = sub.join("a.wav");
        write_sine_wav(&p, 44_100, 2, 0.2, 440.0);
        std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o000)).unwrap();
        let lib = Library::open_in_memory().unwrap();
        let result = add_file(&lib, &p);
        std::fs::set_permissions(&sub, std::fs::Permissions::from_mode(0o700)).unwrap();
        assert_eq!(result.err().unwrap().to_string(), "KaraAlwaysOK isn't allowed to read this file.");
    }

    #[test]
    fn adding_a_non_audio_file_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let text = dir.path().join("x.mp3");
        std::fs::write(&text, "not audio").unwrap();
        let lib = Library::open_in_memory().unwrap();
        let err = add_file(&lib, &text).err().unwrap();
        assert_eq!(err.to_string(), "This file isn't audio we can play.");
    }

    #[test]
    fn recognizes_already_instrumental_titles() {
        for yes in ["Rooftop Static (Instrumental)", "Salt & Static - Karaoke", "Neon Tidewater (Off Vocal)", "Neon Tidewater off-vocal", "Paper Boats [Inst.]", "紙の船 (伴奏)", "紙の船 カラオケ"] {
            assert!(looks_instrumental(&[yes]), "{yes}");
        }
        for no in ["Instinct", "Institute of Static", "Vocal Warmup"] {
            assert!(!looks_instrumental(&[no]), "{no}");
        }
        assert!(looks_instrumental(&["Rooftop Static", "Demos (Instrumentals)"]));
    }

    #[test]
    fn an_instrumental_file_starts_with_the_singer_left_in() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("rooftop-static-instrumental.wav");
        write_sine_wav(&p, 44_100, 2, 0.2, 440.0);
        let lib = Library::open_in_memory().unwrap();
        let t = lib.track(add_file(&lib, &p).unwrap().track_id).unwrap();
        assert_eq!((t.instrumental, t.vocal_removal), (true, 0));
    }
}
