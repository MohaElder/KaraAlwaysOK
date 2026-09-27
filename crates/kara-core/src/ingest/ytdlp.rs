//! yt-dlp: installed on first use (checksum-verified), then used to pull audio from pages.

use crate::assets::sha256_hex;
use crate::store::write_atomic;
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
    if sha256_hex(&bytes) != want {
        bail!("yt-dlp failed its checksum");
    }
    write_atomic(&path, &bytes)?;
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
pub fn download(bin: &Path, url: &str, out_dir: &Path) -> Result<Fetched> {
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
