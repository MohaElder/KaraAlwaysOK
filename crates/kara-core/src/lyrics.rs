//! Synced lyrics: types, LRC parsing, word timing and lookup.

use crate::library::LyricsSource;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub words: Vec<Word>,
}

/// A word never takes longer than this, so a line before a long instrumental
/// break is sung at a natural pace instead of crawling.
const MAX_WORD_MS: i64 = 600;
const LAST_LINE_MS: i64 = 5_000;

fn parse_timestamp(s: &str) -> Option<i64> {
    let (m, rest) = s.split_once(':')?;
    let m: i64 = m.trim().parse().ok()?;
    let (sec, frac) = rest.split_once('.').unwrap_or((rest, ""));
    let sec: i64 = sec.parse().ok()?;
    if !(0..60).contains(&sec) {
        return None;
    }
    let ms = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().ok()? * 100,
        2 => frac.parse::<i64>().ok()? * 10,
        _ => frac.get(..3)?.parse::<i64>().ok()?,
    };
    Some(m * 60_000 + sec * 1_000 + ms)
}

/// Whether any line carries an `[mm:ss.xx]` timestamp.
pub fn is_synced(text: &str) -> bool {
    text.lines().any(|l| {
        l.trim_start()
            .strip_prefix('[')
            .and_then(|s| s.split_once(']'))
            .is_some_and(|(t, _)| parse_timestamp(t).is_some())
    })
}

fn spread_words(text: &str, start: i64, end: i64) -> Vec<Word> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let d = ((end - start) / words.len().max(1) as i64).clamp(1, MAX_WORD_MS);
    words
        .iter()
        .enumerate()
        .map(|(i, w)| Word { start_ms: start + i as i64 * d, end_ms: start + (i as i64 + 1) * d, text: w.to_string() })
        .collect()
}

/// LRC text → timed lines with evenly spread word timings. Metadata tags and
/// blank lines produce no line (a blank line still ends the line before it).
pub fn parse_lrc(text: &str, duration_ms: i64) -> Vec<Line> {
    let mut stamped: Vec<(i64, String)> = Vec::new();
    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut times = Vec::new();
        while let Some(inner) = rest.strip_prefix('[') {
            let Some(end) = inner.find(']') else { break };
            match parse_timestamp(&inner[..end]) {
                Some(t) => times.push(t),
                None => break,
            }
            rest = &inner[end + 1..];
        }
        let words = rest.trim().to_string();
        for t in times {
            stamped.push((t, words.clone()));
        }
    }
    stamped.sort_by_key(|(t, _)| *t);
    let mut lines = Vec::new();
    for (i, (start, text)) in stamped.iter().enumerate() {
        if text.is_empty() {
            continue;
        }
        let end = match stamped.get(i + 1) {
            Some((next, _)) => *next,
            None => duration_ms.max(start + LAST_LINE_MS),
        };
        lines.push(Line { start_ms: *start, end_ms: end, text: text.clone(), words: spread_words(text, *start, end) });
    }
    lines
}

/// A source of synced (LRC) lyrics for a song.
pub trait LyricsFetcher {
    fn fetch(&self, title: &str, artist: &str, album: Option<&str>, duration_s: u64) -> Result<Option<String>>;
}

/// lrclib.net — free, no key.
pub struct Lrclib {
    client: reqwest::blocking::Client,
}

impl Lrclib {
    pub fn new() -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("kara-always-oki/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(10))
            .build()?;
        Ok(Self { client })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrclibHit {
    synced_lyrics: Option<String>,
}

impl LyricsFetcher for Lrclib {
    fn fetch(&self, title: &str, artist: &str, album: Option<&str>, duration_s: u64) -> Result<Option<String>> {
        let mut q = vec![("track_name", title.to_string()), ("artist_name", artist.to_string()), ("duration", duration_s.to_string())];
        if let Some(a) = album {
            q.push(("album_name", a.to_string()));
        }
        let resp = self.client.get("https://lrclib.net/api/get").query(&q).send()?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let hit: LrclibHit = resp.error_for_status()?.json()?;
        Ok(hit.synced_lyrics.filter(|s| is_synced(s)))
    }
}

/// Embedded synced lyrics first, then the online service, else none.
pub fn find(
    embedded: Option<&str>,
    title: &str,
    artist: Option<&str>,
    album: Option<&str>,
    duration_ms: i64,
    fetcher: &dyn LyricsFetcher,
) -> Result<(LyricsSource, Vec<Line>)> {
    if let Some(text) = embedded.filter(|t| is_synced(t)) {
        return Ok((LyricsSource::Embedded, parse_lrc(text, duration_ms)));
    }
    if let Some(artist) = artist {
        if let Some(text) = fetcher.fetch(title, artist, album, (duration_ms / 1000) as u64)? {
            return Ok((LyricsSource::Lrclib, parse_lrc(&text, duration_ms)));
        }
    }
    Ok((LyricsSource::None, Vec::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn parses_lines_and_times() {
        let lines = parse_lrc("[ar:Someone]\n[00:02.00]Counting windows home\n[00:06.50]Every city light\n", 12_000);
        assert_eq!(lines.len(), 2);
        assert_eq!((lines[0].start_ms, lines[0].end_ms), (2_000, 6_500));
        assert_eq!((lines[1].start_ms, lines[1].end_ms), (6_500, 12_000));
        assert_eq!(lines[1].text, "Every city light");
    }

    #[test]
    fn handles_multiple_stamps_precision_and_blank_breaks() {
        let lines = parse_lrc("[00:01.5][00:20.123]Chorus line\n[00:04.00]\n[00:10.00]Verse\n", 30_000);
        let starts: Vec<_> = lines.iter().map(|l| l.start_ms).collect();
        assert_eq!(starts, vec![1_500, 10_000, 20_123]);
        // The blank line at 4.0 s ends the first line early.
        assert_eq!(lines[0].end_ms, 4_000);
    }

    #[test]
    fn words_are_spread_but_never_slower_than_600ms_each() {
        let lines = parse_lrc("[00:00.00]one two three\n[00:30.00]next\n", 40_000);
        let w = &lines[0].words;
        assert_eq!(w.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(), vec!["one", "two", "three"]);
        assert_eq!((w[0].start_ms, w[1].start_ms, w[2].end_ms), (0, 600, 1_800));
        let quick = parse_lrc("[00:00.00]a b\n[00:01.00]c\n", 2_000);
        assert_eq!(quick[0].words[1].end_ms, 1_000);
    }

    #[test]
    fn last_line_without_duration_gets_five_seconds() {
        let lines = parse_lrc("[01:00.00]end\n", 0);
        assert_eq!(lines[0].end_ms, 65_000);
    }

    #[test]
    fn detects_synced_text() {
        assert!(is_synced("[00:01.00]hi"));
        assert!(!is_synced("just words\nno stamps"));
        assert!(!is_synced("[ar:Artist]\nplain"));
    }

    struct Fake(Option<&'static str>, RefCell<u32>);
    impl LyricsFetcher for Fake {
        fn fetch(&self, _: &str, _: &str, _: Option<&str>, _: u64) -> Result<Option<String>> {
            *self.1.borrow_mut() += 1;
            Ok(self.0.map(String::from))
        }
    }

    #[test]
    fn find_prefers_embedded_then_fetches_then_gives_up() {
        let f = Fake(Some("[00:01.00]online"), RefCell::new(0));
        let (src, lines) = find(Some("[00:00.00]embedded"), "t", Some("a"), None, 5_000, &f).unwrap();
        assert_eq!((src, lines[0].text.as_str(), *f.1.borrow()), (LyricsSource::Embedded, "embedded", 0));
        let (src, lines) = find(Some("plain unsynced"), "t", Some("a"), None, 5_000, &f).unwrap();
        assert_eq!((src, lines[0].text.as_str()), (LyricsSource::Lrclib, "online"));
        let none = Fake(None, RefCell::new(0));
        assert_eq!(find(None, "t", Some("a"), None, 5_000, &none).unwrap(), (LyricsSource::None, vec![]));
        // No artist: don't even ask.
        assert_eq!(find(None, "t", None, None, 5_000, &f).unwrap().0, LyricsSource::None);
    }

    #[test]
    #[ignore = "network"]
    fn lrclib_live_lookup_does_not_error() {
        let l = Lrclib::new().unwrap();
        l.fetch("Bohemian Rhapsody", "Queen", None, 355).unwrap();
    }
}
