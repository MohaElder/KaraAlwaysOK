//! Which pasted links we can get audio from, and what to tell the user otherwise.

use crate::problem::Problem;
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

const EXTRACTABLE: &[&str] = &["youtube.com", "youtu.be", "soundcloud.com", "bandcamp.com", "vimeo.com", "archive.org", "mixcloud.com", "b23.tv"];
const STREAMING: &[&str] = &["spotify.com", "music.apple.com", "tidal.com", "deezer.com"];
const AUDIO_EXT: &[&str] = &["mp3", "wav", "flac", "m4a", "aac", "ogg", "aif", "aiff"];

pub(crate) fn host_is(host: &str, domain: &str) -> bool {
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

/// Whether a link is a Bilibili video page.
fn is_bilibili_video(host: &str, url: &Url) -> bool {
    host_is(host, "bilibili.com") && url.path().starts_with("/video/")
}

/// The one link for a YouTube or Bilibili video, whatever form it was pasted in; other links unchanged.
pub fn canonical(url: &Url) -> Url {
    let host = url.host_str().unwrap_or("").trim_end_matches('.').to_ascii_lowercase();
    if is_bilibili_video(&host, url) {
        let id = url.path_segments().into_iter().flatten().nth(1).unwrap_or_default();
        let mut one = Url::parse(&format!("https://www.bilibili.com/video/{id}")).unwrap_or_else(|_| url.clone());
        if let Some((_, part)) = url.query_pairs().find(|(k, _)| k == "p") {
            one.query_pairs_mut().append_pair("p", &part);
        }
        return one;
    }
    let mut path = url.path_segments().into_iter().flatten();
    let id = if host_is(&host, "youtu.be") {
        path.next().map(str::to_string)
    } else if host_is(&host, "youtube.com") {
        match path.next() {
            Some("watch") => url.query_pairs().find(|(k, _)| k == "v").map(|(_, v)| v.into_owned()),
            Some("shorts" | "live" | "embed") => path.next().map(str::to_string),
            _ => None,
        }
    } else {
        None
    };
    match id.filter(|id| !id.is_empty()) {
        Some(id) => Url::parse_with_params("https://www.youtube.com/watch", [("v", id)]).unwrap_or_else(|_| url.clone()),
        None => url.clone(),
    }
}

pub fn verdict(url: &Url) -> LinkVerdict {
    let host = url.host_str().unwrap_or("").trim_end_matches('.').to_ascii_lowercase();
    let ext = url.path().rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if url.path().contains('.') && AUDIO_EXT.contains(&ext.as_str()) {
        LinkVerdict::AudioFile
    } else if EXTRACTABLE.iter().any(|d| host_is(&host, d)) || is_bilibili_video(&host, url) {
        LinkVerdict::Extractable
    } else if STREAMING.iter().any(|d| host_is(&host, d)) {
        LinkVerdict::Streaming
    } else {
        LinkVerdict::Unsupported
    }
}

/// Why a pasted link can't be used, if it can't.
pub fn rejection(url: &Url) -> Option<Problem> {
    match verdict(url) {
        LinkVerdict::AudioFile | LinkVerdict::Extractable => None,
        LinkVerdict::Streaming => Some(Problem::LinkStreaming),
        LinkVerdict::Unsupported => Some(Problem::LinkUnsupported),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> LinkVerdict {
        verdict(&parse_link(s).unwrap())
    }

    #[test]
    fn every_youtube_and_bilibili_video_link_form_becomes_one_link() {
        let watch = "https://www.youtube.com/watch?v=abc123";
        for form in [
            "https://youtu.be/abc123?si=track",
            "youtube.com/watch?v=abc123&t=30&list=PL1",
            "https://music.youtube.com/watch?v=abc123&si=x",
            "https://m.youtube.com/watch?v=abc123",
            "https://www.youtube.com/shorts/abc123?si=x",
            "https://www.youtube.com/live/abc123",
            "https://www.youtube.com/embed/abc123",
            watch,
        ] {
            assert_eq!(canonical(&parse_link(form).unwrap()).as_str(), watch, "{form}");
        }
        for (form, one) in [
            ("https://m.bilibili.com/video/BV1aaaaaaaaa/?spm_id_from=333.1&vd_source=x", "https://www.bilibili.com/video/BV1aaaaaaaaa"),
            ("bilibili.com/video/BV1aaaaaaaaa?p=2&t=30", "https://www.bilibili.com/video/BV1aaaaaaaaa?p=2"),
        ] {
            assert_eq!(canonical(&parse_link(form).unwrap()).as_str(), one, "{form}");
        }
        for other in ["https://soundcloud.com/a/b?si=x", "https://www.youtube.com/playlist?list=PL1", "https://b23.tv/BV1aaaaaaaaa"] {
            assert_eq!(canonical(&parse_link(other).unwrap()).as_str(), other);
        }
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
        assert_eq!(v("https://m.bilibili.com/video/BV1aaaaaaaaa"), LinkVerdict::Extractable);
        assert_eq!(v("https://b23.tv/BV1aaaaaaaaa"), LinkVerdict::Extractable);
        assert_eq!(v("https://space.bilibili.com/2"), LinkVerdict::Unsupported);
        assert_eq!(v("https://example.com/files/song.FLAC?dl=1"), LinkVerdict::AudioFile);
        assert_eq!(v("https://open.spotify.com/track/x"), LinkVerdict::Streaming);
        assert_eq!(v("https://music.apple.com/us/album/x"), LinkVerdict::Streaming);
        assert_eq!(v("https://example.com/page"), LinkVerdict::Unsupported);
        assert_eq!(v("https://notyoutube.com/x"), LinkVerdict::Unsupported);
        assert_eq!(v("https://youtube.com./watch?v=x"), LinkVerdict::Extractable);
    }

    #[test]
    fn links_we_refuse_say_why() {
        let rejection_of = |url: &str| rejection(&parse_link(url).unwrap());
        assert_eq!(rejection_of("https://open.spotify.com/track/x"), Some(Problem::LinkStreaming));
        assert_eq!(rejection_of("https://example.com/page"), Some(Problem::LinkUnsupported));
        assert_eq!(rejection_of("https://youtu.be/x"), None);
    }
}
