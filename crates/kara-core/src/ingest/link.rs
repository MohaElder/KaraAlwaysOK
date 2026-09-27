//! Which pasted links we can get audio from, and what to tell the user otherwise.

use url::Url;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkVerdict {
    /// A direct link to an audio file we can decode.
    AudioFile,
    /// A page yt-dlp can pull audio from.
    Extractable,
    /// A streaming service; its audio can't be downloaded.
    Streaming,
    Unsupported,
}

const EXTRACTABLE: &[&str] = &["youtube.com", "youtu.be", "soundcloud.com", "bandcamp.com", "vimeo.com", "archive.org", "mixcloud.com"];
const STREAMING: &[&str] = &["spotify.com", "music.apple.com", "tidal.com", "deezer.com"];
const AUDIO_EXT: &[&str] = &["mp3", "wav", "flac", "m4a", "aac", "ogg", "aif", "aiff"];

fn host_is(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

/// Text that looks like a link (with or without https://). Search text returns None.
pub fn parse_link(input: &str) -> Option<Url> {
    let s = input.trim();
    if s.is_empty() || s.chars().any(char::is_whitespace) {
        return None;
    }
    let url = if s.starts_with("http://") || s.starts_with("https://") {
        Url::parse(s).ok()?
    } else {
        Url::parse(&format!("https://{s}")).ok()?
    };
    url.host_str().filter(|h| h.contains('.'))?;
    Some(url)
}

pub fn verdict(url: &Url) -> LinkVerdict {
    let host = url.host_str().unwrap_or("").trim_end_matches('.').to_ascii_lowercase();
    let ext = url.path().rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if url.path().contains('.') && AUDIO_EXT.contains(&ext.as_str()) {
        LinkVerdict::AudioFile
    } else if EXTRACTABLE.iter().any(|d| host_is(&host, d)) {
        LinkVerdict::Extractable
    } else if STREAMING.iter().any(|d| host_is(&host, d)) {
        LinkVerdict::Streaming
    } else {
        LinkVerdict::Unsupported
    }
}

/// What to show under the search bar for a link we won't process.
pub fn rejection_message(url: &Url) -> Option<String> {
    let host = url.host_str().unwrap_or("").trim_end_matches('.').trim_start_matches("www.").trim_start_matches("open.");
    match verdict(url) {
        LinkVerdict::AudioFile | LinkVerdict::Extractable => None,
        LinkVerdict::Streaming => Some(format!("{host} links can't be downloaded. Search for the song instead.")),
        LinkVerdict::Unsupported => Some(format!(
            "Can't get audio from {host}. Paste a YouTube, SoundCloud or Bandcamp link, or a direct link to an audio file."
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> LinkVerdict {
        verdict(&parse_link(s).unwrap())
    }

    #[test]
    fn parses_links_but_not_search_text() {
        assert!(parse_link("https://youtu.be/abc").is_some());
        assert!(parse_link("soundcloud.com/artist/song").is_some());
        assert!(parse_link("paper satellites").is_none());
        assert!(parse_link("mina").is_none());
    }

    #[test]
    fn classifies_links() {
        assert_eq!(v("https://www.youtube.com/watch?v=x"), LinkVerdict::Extractable);
        assert_eq!(v("https://m.youtube.com/watch?v=x"), LinkVerdict::Extractable);
        assert_eq!(v("https://artist.bandcamp.com/track/x"), LinkVerdict::Extractable);
        assert_eq!(v("https://example.com/files/song.FLAC?dl=1"), LinkVerdict::AudioFile);
        assert_eq!(v("https://open.spotify.com/track/x"), LinkVerdict::Streaming);
        assert_eq!(v("https://music.apple.com/us/album/x"), LinkVerdict::Streaming);
        assert_eq!(v("https://example.com/page"), LinkVerdict::Unsupported);
        assert_eq!(v("https://notyoutube.com/x"), LinkVerdict::Unsupported);
        assert_eq!(v("https://youtube.com./watch?v=x"), LinkVerdict::Extractable);
    }

    #[test]
    fn rejection_messages_are_plain() {
        let m = rejection_message(&parse_link("https://open.spotify.com/track/x").unwrap()).unwrap();
        assert!(m.contains("spotify.com links can't be downloaded"));
        let m = rejection_message(&parse_link("https://example.com/page").unwrap()).unwrap();
        assert!(m.contains("Can't get audio from example.com"));
        assert!(rejection_message(&parse_link("https://youtu.be/x").unwrap()).is_none());
    }
}
