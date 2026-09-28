//! What a pasted link points to, shown before anything is downloaded.

use super::link::host_is;
use crate::problem::Problem;
use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};
use url::Url;

/// How long `probe` waits for yt-dlp before giving up.
const PROBE_TIMEOUT: Duration = Duration::from_secs(20);
/// How long `search` waits for yt-dlp before giving up.
const SEARCH_TIMEOUT: Duration = Duration::from_secs(15);
/// How many videos `search` asks for.
pub(crate) const SEARCH_RESULTS: usize = 8;
/// How many phrases a site's search suggestions give.
pub(crate) const SUGGESTIONS: usize = 8;

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
    Ok(LinkPreview { title: o.title, channel: o.author_name, duration_ms: o.duration.map(to_ms), thumbnail: o.thumbnail_url })
}

fn to_ms(seconds: f64) -> i64 {
    (seconds * 1000.0).round() as i64
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
    url: Option<String>,
    title: Option<String>,
    channel: Option<String>,
    uploader: Option<String>,
    duration: Option<f64>,
    thumbnail: Option<String>,
    #[serde(default)]
    thumbnails: Vec<Thumbnail>,
    live_status: Option<String>,
}

#[derive(Deserialize)]
struct Thumbnail {
    url: String,
}

impl Dump {
    fn preview(self) -> LinkPreview {
        LinkPreview {
            title: self.title.unwrap_or_else(|| "Unknown song".into()),
            channel: self.channel.or(self.uploader),
            duration_ms: self.duration.map(to_ms),
            thumbnail: self.thumbnail.or_else(|| self.thumbnails.into_iter().next().map(|t| t.url)).map(|t| match t.strip_prefix("http://") {
                Some(rest) => format!("https://{rest}"),
                None => t,
            }),
        }
    }
}

/// A video found by `search`, with the link to add it by.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct SearchHit {
    pub url: String,
    #[serde(flatten)]
    pub preview: LinkPreview,
}

/// A full preview from yt-dlp, without downloading anything.
pub fn probe(bin: &Path, url: &Url) -> Result<LinkPreview> {
    let out = run_with_timeout(bin, &["--dump-json", "--skip-download", "--no-playlist", "--no-warnings", url.as_str()], PROBE_TIMEOUT)?;
    let d: Dump = serde_json::from_str(out.lines().next().unwrap_or(""))?;
    Ok(d.preview())
}

/// The top YouTube videos for `query`, once each and none live or still to come, from
/// yt-dlp's search without downloading anything.
pub fn search(bin: &Path, query: &str) -> Result<Vec<SearchHit>> {
    let target = format!("ytsearch{SEARCH_RESULTS}:{query}");
    let out = run_with_timeout(bin, &["--flat-playlist", "--dump-json", "--skip-download", "--no-warnings", &target], SEARCH_TIMEOUT)?;
    let mut seen = HashSet::new();
    Ok(out
        .lines()
        .filter_map(|line| serde_json::from_str::<Dump>(line).ok())
        .filter(|d| !matches!(d.live_status.as_deref(), Some("is_live" | "is_upcoming")))
        .filter_map(|mut d| Some(SearchHit { url: d.url.take()?, preview: d.preview() }))
        .filter(|hit| seen.insert(hit.url.clone()))
        .collect())
}

/// Runs yt-dlp and returns its output, killing it if it hasn't finished within `timeout`;
/// drains its stdout on a reader thread while polling so a full pipe can't stall it.
fn run_with_timeout(bin: &Path, args: &[&str], timeout: Duration) -> Result<String> {
    let mut child = Command::new(bin).args(args).stdout(Stdio::piped()).stderr(Stdio::null()).spawn()?;
    let mut stdout_pipe = child.stdout.take().context("read yt-dlp output")?;
    let reader = std::thread::spawn(move || {
        let mut buf = Vec::new();
        let _ = stdout_pipe.read_to_end(&mut buf);
        buf
    });
    let deadline = Instant::now() + timeout;
    let status = loop {
        if let Some(status) = child.try_wait()? {
            break status;
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            bail!(Problem::NoSongAtLink);
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    if !status.success() {
        bail!(Problem::NoSongAtLink);
    }
    Ok(String::from_utf8_lossy(&reader.join().unwrap_or_default()).into_owned())
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
             echo '{\"title\":\"Made Up Song\",\"channel\":\"Made Up Channel\",\"duration\":205.4,\"thumbnail\":\"http://i.example/t.jpg\"}'\n",
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
    fn running_ytdlp_gives_up_on_a_hung_one_instead_of_blocking_forever() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("yt-dlp");
        std::fs::write(&bin, "#!/bin/sh\nsleep 5\n").unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let start = std::time::Instant::now();
        let err = run_with_timeout(&bin, &[], Duration::from_millis(200)).unwrap_err();
        assert!(start.elapsed() < Duration::from_secs(2));
        assert_eq!(err.to_string(), "Couldn't find a song at this link.");
    }

    #[test]
    fn running_ytdlp_drains_large_output_instead_of_deadlocking_on_a_full_pipe() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("yt-dlp");
        let padding = "x".repeat(500_000);
        std::fs::write(
            &bin,
            format!(
                "#!/bin/sh\necho '{{\"title\":\"Made Up Song\",\"channel\":\"Made Up Channel\",\"duration\":205.4,\"thumbnail\":\"https://i.example/t.jpg\",\"padding\":\"{padding}\"}}'\n"
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let start = std::time::Instant::now();
        let out = run_with_timeout(&bin, &[], Duration::from_secs(3)).unwrap();
        assert!(start.elapsed() < Duration::from_secs(1));
        assert!(out.starts_with(r#"{"title":"Made Up Song""#));
    }

    #[test]
    fn search_lists_ytdlp_flat_results_once_each_skipping_broken_live_and_upcoming_entries() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("yt-dlp");
        std::fs::write(
            &bin,
            "#!/bin/sh\ncd \"$(dirname \"$0\")\"\nprintf '%s\\n' \"$@\" > args\ncat <<'EOF'\n\
             {\"_type\":\"url\",\"id\":\"aaa\",\"url\":\"https://www.youtube.com/watch?v=aaa\",\"title\":\"Made Up Song\",\"channel\":\"Made Up Channel\",\"duration\":205.4,\"thumbnails\":[{\"url\":\"https://i.example/a-small.jpg\",\"width\":360},{\"url\":\"https://i.example/a-big.jpg\",\"width\":720}]}\n\
             not json\n\
             {\"_type\":\"url\",\"id\":\"bbb\",\"title\":\"No Link\"}\n\
             {\"_type\":\"url\",\"id\":\"ccc\",\"url\":\"https://www.youtube.com/watch?v=ccc\",\"title\":\"Old Stream\",\"uploader\":\"Someone\",\"duration\":null,\"live_status\":\"was_live\"}\n\
             {\"_type\":\"url\",\"id\":\"ddd\",\"url\":\"https://www.youtube.com/watch?v=ddd\",\"title\":\"On Air\",\"live_status\":\"is_live\"}\n\
             {\"_type\":\"url\",\"id\":\"eee\",\"url\":\"https://www.youtube.com/watch?v=eee\",\"title\":\"Premiere\",\"live_status\":\"is_upcoming\"}\n\
             {\"_type\":\"url\",\"id\":\"aaa\",\"url\":\"https://www.youtube.com/watch?v=aaa\",\"title\":\"Made Up Song\"}\n\
             EOF\n",
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let hits = search(&bin, "paper boats").unwrap();
        assert_eq!(
            hits,
            vec![
                SearchHit {
                    url: "https://www.youtube.com/watch?v=aaa".into(),
                    preview: LinkPreview { title: "Made Up Song".into(), channel: Some("Made Up Channel".into()), duration_ms: Some(205_400), thumbnail: Some("https://i.example/a-small.jpg".into()) },
                },
                SearchHit {
                    url: "https://www.youtube.com/watch?v=ccc".into(),
                    preview: LinkPreview { title: "Old Stream".into(), channel: Some("Someone".into()), duration_ms: None, thumbnail: None },
                },
            ]
        );
        let args = std::fs::read_to_string(dir.path().join("args")).unwrap();
        assert!(args.lines().any(|a| a == "ytsearch8:paper boats") && args.contains("--flat-playlist") && args.contains("--skip-download"));
    }

    #[test]
    #[ignore = "network"]
    fn search_live_lookup_finds_videos() {
        let hits = search(Path::new("yt-dlp"), "lofi piano instrumental").unwrap();
        assert!(!hits.is_empty() && hits.iter().all(|h| h.url.starts_with("https://www.youtube.com/")));
    }

    #[test]
    #[ignore = "network"]
    fn oembed_live_lookup_finds_a_real_title() {
        let url = Url::parse("https://www.youtube.com/watch?v=JGwWNGJdvx8").unwrap();
        let p = oembed(&url).unwrap().unwrap();
        assert!(!p.title.is_empty());
    }
}
