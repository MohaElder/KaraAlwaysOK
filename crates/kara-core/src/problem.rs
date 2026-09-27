//! User-visible problems, as stable codes the app translates and the CLI prints in English.

use serde::Serialize;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum Problem {
    NewerLibrary,
    UpgradeFailed,
    DataFolder,
    InUse,
    LibraryOpen,
    EngineDownload,
    EngineStart,
    NothingPlaying,
    NotALink,
    FileMoved,
    FileNotAllowed,
    ReadFailed,
    NotAudio,
    SongGone,
    NoAudio,
    Download,
    DownloaderSetup,
    Unreadable,
    SongAudio,
    Empty,
    DiskFull,
    Save,
    Separate,
    NotPrepared,
    PartNotReady,
    StreamingLater,
    LinkStreaming,
    LinkUnsupported,
    NoSongAtLink,
    PhonesStart,
    NoNetwork,
    LyricsLookup,
}

impl std::fmt::Display for Problem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::NewerLibrary => "This library was made by a newer version of KaraAlwaysOK. Update KaraAlwaysOK to open it.",
            Self::UpgradeFailed => "This library couldn't be upgraded safely.",
            Self::DataFolder => "Couldn't open KaraAlwaysOK's data folder.",
            Self::InUse => "Another copy of KaraAlwaysOK is using your library. Close it and try again.",
            Self::LibraryOpen => "Couldn't open your library.",
            Self::EngineDownload => "Couldn't download the singing engine. Check your connection.",
            Self::EngineStart => "Couldn't start the singing engine.",
            Self::NothingPlaying => "Nothing is playing.",
            Self::NotALink => "That's not a link.",
            Self::FileMoved => "The file was moved or deleted.",
            Self::FileNotAllowed => "KaraAlwaysOK isn't allowed to read this file.",
            Self::ReadFailed => "Couldn't read this file.",
            Self::NotAudio => "This file isn't audio we can play.",
            Self::SongGone => "This song is no longer in your library.",
            Self::NoAudio => "This song has no audio to play.",
            Self::Download => "Couldn't download this song. Check the link and your connection.",
            Self::DownloaderSetup => "Couldn't set up downloading. Check your connection.",
            Self::Unreadable => "Couldn't read this audio. The format may not be supported.",
            Self::SongAudio => "Couldn't read this song's audio.",
            Self::Empty => "This audio is empty.",
            Self::DiskFull => "Couldn't save the audio. The disk is full.",
            Self::Save => "Couldn't save the audio.",
            Self::Separate => "Couldn't take the vocals out of this song.",
            Self::NotPrepared => "This song isn't prepared yet.",
            Self::PartNotReady => "This part of the song isn't ready yet.",
            Self::StreamingLater => "Songs from streaming libraries arrive in a later version.",
            Self::LinkStreaming => "Links from streaming services can't be downloaded. Search for the song instead.",
            Self::LinkUnsupported => "Can't get audio from this link. Paste a YouTube, SoundCloud or Bandcamp link, or a direct link to an audio file.",
            Self::NoSongAtLink => "Couldn't find a song at this link.",
            Self::PhonesStart => "Couldn't start phone mics.",
            Self::NoNetwork => "Connect this computer to Wi-Fi to use phone mics.",
            Self::LyricsLookup => "Couldn't look for lyrics. Check your connection.",
        })
    }
}

impl std::error::Error for Problem {}

/// The problem an error carries, if any (the outermost one when there are several).
pub fn problem(e: &anyhow::Error) -> Option<Problem> {
    e.downcast_ref::<Problem>().copied()
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    const SENTENCE_STARTS: &[&str] = &["\"Couldn", "\"Can't", "\"This ", "\"The ", "\"That", "\"Another", "\"Songs", "\"Links", "\"KaraAlwaysOK"];

    fn visit(dir: &Path, f: &mut dyn FnMut(&Path, &str)) {
        for entry in std::fs::read_dir(dir).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                visit(&p, f);
            } else if p.extension().is_some_and(|x| x == "rs") {
                f(&p, &std::fs::read_to_string(&p).unwrap());
            }
        }
    }

    #[test]
    fn user_visible_errors_all_come_from_problem() {
        let mut left = Vec::new();
        visit(Path::new(concat!(env!("CARGO_MANIFEST_DIR"), "/src")), &mut |path, text| {
            if path.ends_with("problem.rs") {
                return;
            }
            let code = text.split("#[cfg(test)]\nmod ").next().unwrap_or("");
            for (i, line) in code.lines().enumerate() {
                if !line.trim_start().starts_with("//") && SENTENCE_STARTS.iter().any(|s| line.contains(s)) {
                    left.push(format!("{}:{}: {}", path.display(), i + 1, line.trim()));
                }
            }
        });
        assert!(left.is_empty(), "user-visible text without a Problem code:\n{}", left.join("\n"));
    }
}
