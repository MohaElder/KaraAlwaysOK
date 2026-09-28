//! yt-dlp: installed on first use (checksum-verified), then used to pull audio from pages.

use crate::assets::install_verified;
use crate::lyrics::name_guesses;
use crate::store::write_atomic;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

const ZIP_NAME: &str = "yt-dlp_macos.zip";
const ZIP_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos.zip";
const SUMS_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/SHA2-256SUMS";
const EXE_NAME: &str = "yt-dlp_macos";
/// Names the folder of the install in use, by the checksum of the zip it came from.
const CURRENT: &str = "yt-dlp.current";

static INSTALL: Mutex<()> = Mutex::new(());

pub fn sha_from_sums(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (hash, file) = l.split_once(char::is_whitespace)?;
        (file.trim() == name).then(|| hash.to_string())
    })
}

fn get(url: &str) -> Result<Vec<u8>> {
    Ok(reqwest::blocking::get(url)?.error_for_status()?.bytes()?.to_vec())
}

fn latest_sha() -> Result<String> {
    let sums = String::from_utf8(get(SUMS_URL)?)?;
    sha_from_sums(&sums, ZIP_NAME).context("yt-dlp checksum not published")
}

fn version_dir(bin_dir: &Path, sha: &str) -> PathBuf {
    bin_dir.join(format!("yt-dlp-{sha}"))
}

fn current_sha(bin_dir: &Path) -> Option<String> {
    std::fs::read_to_string(bin_dir.join(CURRENT)).ok()
}

/// Path to a verified yt-dlp in `bin_dir`, installing its fast-starting (unpacked) form on first use.
pub fn ensure(bin_dir: &Path) -> Result<PathBuf> {
    let _one = INSTALL.lock().unwrap_or_else(|e| e.into_inner());
    if let Some(exe) = current_sha(bin_dir).map(|sha| version_dir(bin_dir, &sha).join(EXE_NAME)).filter(|exe| exe.exists()) {
        return Ok(exe);
    }
    unpack(bin_dir, &get(ZIP_URL)?, &latest_sha()?)
}

/// Installs the newest yt-dlp beside the current one when a newer release is out; true when it did.
pub fn update(bin_dir: &Path) -> Result<bool> {
    let _one = INSTALL.lock().unwrap_or_else(|e| e.into_inner());
    let want = latest_sha()?;
    if current_sha(bin_dir).is_some_and(|have| have == want) {
        return Ok(false);
    }
    unpack(bin_dir, &get(ZIP_URL)?, &want)?;
    Ok(true)
}

/// Verifies the release zip against `sha`, unpacks it into its own folder and makes it the
/// current install, leaving earlier installs in place; returns the executable's path.
fn unpack(bin_dir: &Path, zip: &[u8], sha: &str) -> Result<PathBuf> {
    std::fs::create_dir_all(bin_dir)?;
    let zip_path = install_verified(bin_dir, ZIP_NAME, zip, sha)?;
    let staging = bin_dir.join("yt-dlp.new");
    let _ = std::fs::remove_dir_all(&staging);
    let status = Command::new("/usr/bin/ditto").args(["-x", "-k"]).arg(&zip_path).arg(&staging).status();
    std::fs::remove_file(&zip_path)?;
    if !status?.success() {
        bail!("couldn't unpack yt-dlp");
    }
    let dir = version_dir(bin_dir, sha);
    if dir.exists() {
        std::fs::remove_dir_all(&staging)?;
    } else {
        std::fs::rename(&staging, &dir)?;
    }
    write_atomic(&bin_dir.join(CURRENT), sha.as_bytes())?;
    Ok(dir.join(EXE_NAME))
}

/// Removes every yt-dlp install but the current one (the old single-file one included);
/// only for startup, when none of them can be running.
pub fn remove_old_installs(bin_dir: &Path) {
    let keep = current_sha(bin_dir).map(|sha| format!("yt-dlp-{sha}"));
    let Ok(entries) = std::fs::read_dir(bin_dir) else { return };
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        if !name.starts_with("yt-dlp") || name == CURRENT || Some(&name) == keep.as_ref() {
            continue;
        }
        let path = entry.path();
        let _ = if path.is_dir() { std::fs::remove_dir_all(&path) } else { std::fs::remove_file(&path) };
    }
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
    pub tags: Vec<String>,
    pub thumbnail: Option<String>,
    /// Likely (title, singer) pairs when the video's music fields don't name both.
    pub guesses: Vec<(String, String)>,
    pub duration_s: u64,
}

#[derive(Deserialize)]
struct Info {
    filepath: PathBuf,
    title: Option<String>,
    uploader: Option<String>,
    artist: Option<String>,
    track: Option<String>,
    album: Option<String>,
    tags: Option<Vec<String>>,
    thumbnail: Option<String>,
    duration: Option<f64>,
}

/// Downloads the best audio we can decode (M4A, else MP3, else whatever is best).
/// If that fails and `update` brings a newer yt-dlp, tries once more.
pub fn download(bin: &Path, url: &str, out_dir: &Path, update: impl FnOnce() -> Result<bool>) -> Result<Fetched> {
    download_once(bin, url, out_dir).or_else(|e| {
        if update().unwrap_or(false) {
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
        .args(["--print", "after_move:%(.{filepath,title,uploader,artist,track,album,tags,thumbnail,duration})j"])
        .arg(url)
        .output()
        .context("run yt-dlp")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("{}", err.lines().last().unwrap_or("yt-dlp failed"));
    }
    let line = String::from_utf8_lossy(&out.stdout).lines().last().unwrap_or("").to_string();
    let info: Info = serde_json::from_str(&line).context("read yt-dlp output")?;
    let video = info.title.as_deref().unwrap_or("Unknown song");
    let guesses = if info.track.is_some() && info.artist.is_some() { Vec::new() } else { name_guesses(video, info.uploader.as_deref()) };
    let (title, artist) = match info.track {
        Some(track) => (track, info.artist.or(info.uploader)),
        None => clean_meta(video, info.uploader.as_deref()),
    };
    let duration_s = info.duration.unwrap_or(0.0).round() as u64;
    Ok(Fetched { path: info.filepath, title, artist, album: info.album, tags: info.tags.unwrap_or_default(), thumbnail: info.thumbnail, guesses, duration_s })
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

    /// A stand-in yt-dlp whose download succeeds only once a file named "updated" exists.
    fn fake_ytdlp(dir: &Path) -> PathBuf {
        let bin = dir.join("yt-dlp");
        let script = "#!/bin/sh\ncd \"$(dirname \"$0\")\"\necho x >> attempts\n\
             [ -f updated ] || { echo 'ERROR: site changed' >&2; exit 1; }\n\
             echo '{\"filepath\":\"/made/up.m4a\",\"title\":\"Made Up Song\",\"uploader\":\"Made Up Channel\",\"tags\":[\"made up tag\"]}'\n";
        std::fs::write(&bin, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    #[test]
    fn a_failed_download_retries_once_when_an_update_brings_a_newer_ytdlp() {
        let dir = tempfile::tempdir().unwrap();
        let bin = fake_ytdlp(dir.path());
        let f = download(&bin, "https://youtu.be/x", dir.path(), || {
            std::fs::write(dir.path().join("updated"), "")?;
            Ok(true)
        })
        .unwrap();
        assert_eq!((f.title.as_str(), f.artist.as_deref()), ("Made Up Song", Some("Made Up Channel")));
        assert_eq!(f.tags, vec!["made up tag".to_string()]);

        let dir = tempfile::tempdir().unwrap();
        let bin = fake_ytdlp(dir.path());
        assert!(download(&bin, "https://youtu.be/x", dir.path(), || Ok(false)).is_err());
        assert_eq!(std::fs::read_to_string(dir.path().join("attempts")).unwrap().lines().count(), 1);
    }

    /// A zip like the release's, whose executable prints `version`.
    fn fixture_zip(dir: &Path, version: &str) -> Vec<u8> {
        let src = dir.join(format!("src-{version}"));
        std::fs::create_dir_all(src.join("_internal")).unwrap();
        std::fs::write(src.join("_internal/lib.txt"), "made up").unwrap();
        std::fs::write(src.join(EXE_NAME), format!("#!/bin/sh\necho {version}\n")).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(src.join(EXE_NAME), std::fs::Permissions::from_mode(0o755)).unwrap();
        let zip_path = dir.join(format!("{version}.zip"));
        assert!(Command::new("/usr/bin/ditto").args(["-c", "-k"]).arg(&src).arg(&zip_path).status().unwrap().success());
        std::fs::read(zip_path).unwrap()
    }

    #[test]
    fn each_version_unpacks_into_its_own_folder_and_old_ones_go_only_at_startup_cleanup() {
        let dir = tempfile::tempdir().unwrap();
        let run = |exe: &Path| String::from_utf8(Command::new(exe).output().unwrap().stdout).unwrap();
        let (v1, v2) = (fixture_zip(dir.path(), "1"), fixture_zip(dir.path(), "2"));
        let sha = crate::assets::sha256_hex;
        let bin_dir = dir.path().join("bin");
        std::fs::create_dir_all(&bin_dir).unwrap();
        std::fs::write(bin_dir.join("yt-dlp"), "old single file").unwrap();

        assert!(unpack(&bin_dir, &v1, "0000").is_err());
        let exe1 = unpack(&bin_dir, &v1, &sha(&v1)).unwrap();
        assert_eq!(run(&exe1), "1\n");
        assert!(bin_dir.join("yt-dlp").is_file());

        let not_a_zip = b"not a zip".to_vec();
        assert!(unpack(&bin_dir, &not_a_zip, &sha(&not_a_zip)).is_err());
        assert_eq!(ensure(&bin_dir).unwrap(), exe1);

        let exe2 = unpack(&bin_dir, &v2, &sha(&v2)).unwrap();
        assert_eq!((run(&exe1), run(&exe2)), ("1\n".to_string(), "2\n".to_string()));
        assert_eq!(ensure(&bin_dir).unwrap(), exe2);

        remove_old_installs(&bin_dir);
        let mut left: Vec<_> = std::fs::read_dir(&bin_dir).unwrap().map(|e| e.unwrap().file_name().into_string().unwrap()).collect();
        left.sort();
        assert_eq!(left, [format!("yt-dlp-{}", sha(&v2)), CURRENT.to_string()]);
        assert_eq!(run(&exe2), "2\n");
    }

    #[test]
    #[ignore = "network"]
    fn installs_the_fast_starting_ytdlp_and_runs_it() {
        let dir = tempfile::tempdir().unwrap();
        let exe = ensure(dir.path()).unwrap();
        let out = Command::new(&exe).arg("--version").output().unwrap();
        assert!(out.status.success() && !out.stdout.is_empty());
        assert!(!update(dir.path()).unwrap());
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
