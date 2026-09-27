//! yt-dlp: installed on first use (checksum-verified), then used to pull audio from pages.

use crate::assets::install_verified;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

const BIN_NAME: &str = "yt-dlp_macos";
const BIN_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos";
const SUMS_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/SHA2-256SUMS";

pub fn sha_from_sums(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (hash, file) = l.split_once(char::is_whitespace)?;
        (file.trim() == name).then(|| hash.to_string())
    })
}

/// Path to a verified yt-dlp in `bin_dir`, installing it on first use.
pub fn ensure(bin_dir: &Path) -> Result<PathBuf> {
    let path = bin_dir.join("yt-dlp");
    if path.exists() {
        return Ok(path);
    }
    let get = |url: &str| -> Result<Vec<u8>> {
        Ok(reqwest::blocking::get(url)?.error_for_status()?.bytes()?.to_vec())
    };
    let sums = String::from_utf8(get(SUMS_URL)?)?;
    let want = sha_from_sums(&sums, BIN_NAME).context("yt-dlp checksum not published")?;
    let bytes = get(BIN_URL)?;
    let path = install_verified(bin_dir, "yt-dlp", &bytes, &want)?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

const NOISE: &[&str] = &["official", "lyric", "lyrics", "audio", "video", "mv", "hd", "4k", "visualizer"];

fn strip_noise(s: &str) -> String {
    let mut out = s.trim().to_string();
    loop {
        let t = out.trim_end();
        let Some(close) = t.chars().last().filter(|c| *c == ')' || *c == ']') else { break };
        let open = if close == ')' { '(' } else { '[' };
        let Some(i) = t.rfind(open) else { break };
        let inner = t[i + 1..t.len() - 1].to_lowercase();
        if inner.split(|c: char| !c.is_alphanumeric()).any(|w| NOISE.contains(&w)) {
            out = t[..i].trim_end().to_string();
        } else {
            break;
        }
    }
    out
}

/// Video title + channel to (song title, artist). Handles "Artist - Title (Official Video)"
/// and YouTube Music's "Artist - Topic" channels.
pub fn clean_meta(title: &str, uploader: Option<&str>) -> (String, Option<String>) {
    let title = strip_noise(title);
    if let Some((artist, song)) = title.split_once(" - ") {
        return (strip_noise(song), Some(artist.trim().to_string()));
    }
    (title, uploader.map(|u| u.trim_end_matches(" - Topic").trim().to_string()))
}

pub struct Fetched {
    pub path: PathBuf,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
}

#[derive(Deserialize)]
struct Info {
    filepath: PathBuf,
    title: Option<String>,
    uploader: Option<String>,
    artist: Option<String>,
    track: Option<String>,
    album: Option<String>,
}

/// Downloads the best audio we can decode (M4A, else MP3, else whatever is best).
/// If that fails, updates yt-dlp and tries once more.
pub fn download(bin: &Path, url: &str, out_dir: &Path) -> Result<Fetched> {
    download_once(bin, url, out_dir).or_else(|e| {
        let updated = Command::new(bin).arg("-U").output().is_ok_and(|o| o.status.success());
        if updated {
            download_once(bin, url, out_dir)
        } else {
            Err(e)
        }
    })
}

fn download_once(bin: &Path, url: &str, out_dir: &Path) -> Result<Fetched> {
    std::fs::create_dir_all(out_dir)?;
    let out = Command::new(bin)
        .args(["--no-playlist", "--no-progress", "-f", "bestaudio[ext=m4a]/bestaudio[ext=mp3]/bestaudio"])
        .arg("-o")
        .arg(out_dir.join("%(id)s.%(ext)s"))
        .args(["--print", "after_move:%(.{filepath,title,uploader,artist,track,album})j"])
        .arg(url)
        .output()
        .context("run yt-dlp")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("{}", err.lines().last().unwrap_or("yt-dlp failed"));
    }
    let line = String::from_utf8_lossy(&out.stdout).lines().last().unwrap_or("").to_string();
    let info: Info = serde_json::from_str(&line).context("read yt-dlp output")?;
    let (title, artist) = match info.track {
        Some(track) => (track, info.artist.or(info.uploader)),
        None => clean_meta(info.title.as_deref().unwrap_or("Unknown song"), info.uploader.as_deref()),
    };
    Ok(Fetched { path: info.filepath, title, artist, album: info.album })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_checksum_in_sums_file() {
        let sums = "aaa  yt-dlp\nbbb  yt-dlp_macos\nccc  yt-dlp_macos.zip\n";
        assert_eq!(sha_from_sums(sums, "yt-dlp_macos").as_deref(), Some("bbb"));
        assert_eq!(sha_from_sums(sums, "nope"), None);
    }

    /// A stand-in yt-dlp: `-U` runs `update`; a download succeeds only once a file named "updated" exists.
    fn fake_ytdlp(dir: &Path, update: &str) -> PathBuf {
        let bin = dir.join("yt-dlp");
        let script = format!(
            "#!/bin/sh\ncd \"$(dirname \"$0\")\"\nif [ \"$1\" = -U ]; then {update}; fi\necho x >> attempts\n\
             [ -f updated ] || {{ echo 'ERROR: site changed' >&2; exit 1; }}\n\
             echo '{{\"filepath\":\"/made/up.m4a\",\"title\":\"Made Up Song\",\"uploader\":\"Made Up Channel\"}}'\n"
        );
        std::fs::write(&bin, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    #[test]
    fn a_failed_download_updates_ytdlp_and_retries_once() {
        let dir = tempfile::tempdir().unwrap();
        let bin = fake_ytdlp(dir.path(), "touch updated; exit 0");
        let f = download(&bin, "https://youtu.be/x", dir.path()).unwrap();
        assert_eq!((f.title.as_str(), f.artist.as_deref()), ("Made Up Song", Some("Made Up Channel")));

        let dir = tempfile::tempdir().unwrap();
        let bin = fake_ytdlp(dir.path(), "exit 0");
        assert!(download(&bin, "https://youtu.be/x", dir.path()).is_err());
        assert_eq!(std::fs::read_to_string(dir.path().join("attempts")).unwrap().lines().count(), 2);
    }

    #[test]
    fn cleans_video_titles() {
        assert_eq!(
            clean_meta("Kiko & the Late Shift - Midnight Laundromat (Official Video)", Some("KikoVEVO")),
            ("Midnight Laundromat".to_string(), Some("Kiko & the Late Shift".to_string()))
        );
        assert_eq!(clean_meta("Overpass Karaoke", Some("Dani Sato - Topic")), ("Overpass Karaoke".to_string(), Some("Dani Sato".to_string())));
        assert_eq!(clean_meta("Song [Lyrics] (HD)", None), ("Song".to_string(), None));
        assert_eq!(clean_meta("Song (feat. Someone)", None).0, "Song (feat. Someone)");
    }
}
