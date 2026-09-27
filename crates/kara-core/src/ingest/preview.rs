//! What a pasted link points to, shown before anything is downloaded.

use super::link::host_is;
use anyhow::{bail, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;
use std::time::Duration;
use url::Url;

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LinkPreview {
    pub title: String,
    pub channel: Option<String>,
    pub duration_ms: Option<i64>,
    pub thumbnail: Option<String>,
}

/// The oEmbed endpoint for a link, for sites that offer one.
pub fn oembed_endpoint(url: &Url) -> Option<Url> {
    let host = url.host_str()?.trim_end_matches('.').to_ascii_lowercase();
    let base = if host_is(&host, "youtube.com") || host_is(&host, "youtu.be") {
        "https://www.youtube.com/oembed"
    } else if host_is(&host, "soundcloud.com") {
        "https://soundcloud.com/oembed"
    } else if host_is(&host, "vimeo.com") {
        "https://vimeo.com/api/oembed.json"
    } else {
        return None;
    };
    Url::parse_with_params(base, [("format", "json"), ("url", url.as_str())]).ok()
}

#[derive(Deserialize)]
struct OEmbed {
    title: String,
    author_name: Option<String>,
    thumbnail_url: Option<String>,
    duration: Option<f64>,
}

pub fn parse_oembed(json: &str) -> Result<LinkPreview> {
    let o: OEmbed = serde_json::from_str(json)?;
    Ok(LinkPreview { title: o.title, channel: o.author_name, duration_ms: o.duration.map(|s| (s * 1000.0) as i64), thumbnail: o.thumbnail_url })
}

/// A quick preview from the site's oEmbed endpoint; `None` for sites without one.
pub fn oembed(url: &Url) -> Result<Option<LinkPreview>> {
    let Some(endpoint) = oembed_endpoint(url) else { return Ok(None) };
    let client = reqwest::blocking::Client::builder().timeout(Duration::from_secs(8)).build()?;
    let body = client.get(endpoint).send()?.error_for_status()?.text()?;
    parse_oembed(&body).map(Some)
}

#[derive(Deserialize)]
struct Dump {
    title: Option<String>,
    channel: Option<String>,
    uploader: Option<String>,
    duration: Option<f64>,
    thumbnail: Option<String>,
}

/// A full preview from yt-dlp, without downloading anything.
pub fn probe(bin: &Path, url: &Url) -> Result<LinkPreview> {
    let out = Command::new(bin)
        .args(["--dump-json", "--skip-download", "--no-playlist", "--no-warnings"])
        .arg(url.as_str())
        .output()?;
    if !out.status.success() {
        bail!("Couldn't find a song at this link.");
    }
    let stdout = String::from_utf8_lossy(&out.stdout);
    let d: Dump = serde_json::from_str(stdout.lines().next().unwrap_or(""))?;
    Ok(LinkPreview {
        title: d.title.unwrap_or_else(|| "Unknown song".into()),
        channel: d.channel.or(d.uploader),
        duration_ms: d.duration.map(|s| (s * 1000.0).round() as i64),
        thumbnail: d.thumbnail,
    })
}

/// A preview for a direct link to an audio file, from its name.
pub fn file_preview(url: &Url) -> LinkPreview {
    let name = url.path_segments().and_then(|mut s| s.next_back()).unwrap_or("");
    LinkPreview {
        title: super::title_from_file_name(name),
        channel: url.host_str().map(|h| h.trim_start_matches("www.").to_string()),
        duration_ms: None,
        thumbnail: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn oembed_gives_title_channel_and_thumbnail() {
        let yt = Url::parse("https://youtu.be/abc?t=3").unwrap();
        assert_eq!(
            oembed_endpoint(&yt).unwrap().as_str(),
            "https://www.youtube.com/oembed?format=json&url=https%3A%2F%2Fyoutu.be%2Fabc%3Ft%3D3"
        );
        assert!(oembed_endpoint(&Url::parse("https://artist.bandcamp.com/track/x").unwrap()).is_none());
        let p = parse_oembed(r#"{"title":"Made Up Song","author_name":"Made Up Channel","thumbnail_url":"https://i.example/t.jpg","type":"video"}"#).unwrap();
        assert_eq!((p.title.as_str(), p.channel.as_deref(), p.duration_ms), ("Made Up Song", Some("Made Up Channel"), None));
        assert_eq!(parse_oembed(r#"{"title":"Clip","author_name":"Someone","duration":61}"#).unwrap().duration_ms, Some(61_000));
    }

    #[test]
    fn probe_reads_details_from_ytdlp_without_downloading() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("yt-dlp");
        std::fs::write(
            &bin,
            "#!/bin/sh\ncd \"$(dirname \"$0\")\"\necho \"$@\" > args\n\
             echo '{\"title\":\"Made Up Song\",\"channel\":\"Made Up Channel\",\"duration\":205.4,\"thumbnail\":\"https://i.example/t.jpg\"}'\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let p = probe(&bin, &Url::parse("https://youtu.be/x").unwrap()).unwrap();
        assert_eq!(
            p,
            LinkPreview { title: "Made Up Song".into(), channel: Some("Made Up Channel".into()), duration_ms: Some(205_400), thumbnail: Some("https://i.example/t.jpg".into()) }
        );
        assert!(std::fs::read_to_string(dir.path().join("args")).unwrap().contains("--skip-download"));
    }

    #[test]
    #[ignore = "network"]
    fn oembed_live_lookup_finds_a_real_title() {
        let url = Url::parse("https://www.youtube.com/watch?v=JGwWNGJdvx8").unwrap();
        let p = oembed(&url).unwrap().unwrap();
        assert!(!p.title.is_empty());
    }
}
