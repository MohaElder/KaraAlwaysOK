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

/// Who sings a duet line.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Voice {
    M,
    F,
    Both,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub words: Vec<Word>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub voice: Option<Voice>,
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

fn strip_bom(text: &str) -> &str {
    text.strip_prefix('\u{FEFF}').unwrap_or(text)
}

/// Whether any line carries an `[mm:ss.xx]` timestamp.
pub fn is_synced(text: &str) -> bool {
    strip_bom(text).lines().any(|l| {
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

/// "M: words" → (Some(Voice::M), "words"); text without a voice marker is returned as is.
fn split_voice(text: &str) -> (Option<Voice>, &str) {
    let Some((tag, rest)) = text.split_once(':') else { return (None, text) };
    let voice = match tag.trim().to_ascii_lowercase().as_str() {
        "m" | "male" | "v1" => Voice::M,
        "f" | "female" | "v2" => Voice::F,
        "d" | "duet" | "both" | "all" | "v1000" => Voice::Both,
        _ => return (None, text),
    };
    (Some(voice), rest.trim())
}

/// LRC text → timed lines with evenly spread word timings and duet voices.
/// Metadata tags and blank lines produce no line (a blank line still ends the line before it).
pub fn parse_lrc(text: &str, duration_ms: i64) -> Vec<Line> {
    let mut stamped: Vec<(i64, String, Option<Voice>)> = Vec::new();
    let mut voice = None;
    for raw in strip_bom(text).lines() {
        let mut rest = raw.trim();
        let mut times = Vec::new();
        while let Some(inner) = rest.strip_prefix('[') {
            let Some(end) = inner.find(']') else { break };
            match parse_timestamp(&inner[..end]) {
                Some(t) => times.push(t),
                None if times.is_empty() => break,
                None => {}
            }
            rest = &inner[end + 1..];
        }
        let (marker, words) = split_voice(rest.trim());
        if marker.is_some() {
            voice = marker;
        }
        for t in times {
            stamped.push((t, words.to_string(), voice));
        }
    }
    stamped.sort_by_key(|(t, ..)| *t);
    let mut lines = Vec::new();
    for (i, (start, text, voice)) in stamped.iter().enumerate() {
        if text.is_empty() {
            continue;
        }
        let end = match stamped.get(i + 1) {
            Some((next, ..)) => *next,
            None => duration_ms.max(start + LAST_LINE_MS),
        };
        lines.push(Line { start_ms: *start, end_ms: end, text: text.clone(), words: spread_words(text, *start, end), voice: *voice });
    }
    lines
}

/// A source of synced (LRC) lyrics for a song.
pub trait LyricsFetcher {
    /// Without an artist, matches by title and duration.
    fn fetch(&self, title: &str, artist: Option<&str>, album: Option<&str>, duration_s: u64) -> Result<Option<String>>;
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
    duration: f64,
}

/// The synced lyrics of the only hit within 1 s of `duration_s`; none if zero or several match.
fn sole_synced_match(hits: Vec<LrclibHit>, duration_s: u64) -> Option<String> {
    let mut near = hits
        .into_iter()
        .filter(|h| (h.duration - duration_s as f64).abs() <= 1.0)
        .filter_map(|h| h.synced_lyrics.filter(|s| is_synced(s)));
    let only = near.next()?;
    near.next().is_none().then_some(only)
}

impl LyricsFetcher for Lrclib {
    fn fetch(&self, title: &str, artist: Option<&str>, album: Option<&str>, duration_s: u64) -> Result<Option<String>> {
        let Some(artist) = artist else {
            let resp = self.client.get("https://lrclib.net/api/search").query(&[("track_name", title)]).send()?;
            return Ok(sole_synced_match(resp.error_for_status()?.json()?, duration_s));
        };
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
    if let Some(text) = fetcher.fetch(title, artist, album, (duration_ms / 1000) as u64)? {
        return Ok((LyricsSource::Lrclib, parse_lrc(&text, duration_ms)));
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
        // A stray non-timestamp tag after a real stamp is dropped, not kept as text.
        let stray = parse_lrc("[00:01.00][01:02:50]hi\n[00:02.00]next\n", 3_000);
        assert_eq!(stray[0].text, "hi");
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
        fn fetch(&self, _: &str, _: Option<&str>, _: Option<&str>, _: u64) -> Result<Option<String>> {
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
        // No artist: still asks, by title.
        assert_eq!(find(None, "t", None, None, 5_000, &f).unwrap().0, LyricsSource::Lrclib);
        // A leading UTF-8 BOM doesn't stop embedded text from being recognized as synced.
        let (src, lines) = find(Some("\u{FEFF}[00:00.00]bommed"), "t", Some("a"), None, 5_000, &f).unwrap();
        assert_eq!((src, lines[0].text.as_str()), (LyricsSource::Embedded, "bommed"));
    }

    #[test]
    fn title_only_search_needs_exactly_one_synced_hit_within_a_second() {
        let hit = |duration, lyrics: Option<&str>| LrclibHit { duration, synced_lyrics: lyrics.map(String::from) };
        let hits = || {
            vec![
                hit(180.0, None),
                hit(179.5, Some("plain, not synced")),
                hit(181.0, Some("[00:01.00]only")),
                hit(182.5, Some("[00:01.00]too far")),
            ]
        };
        assert_eq!(sole_synced_match(hits(), 180).as_deref(), Some("[00:01.00]only"));
        assert_eq!(sole_synced_match(hits(), 190), None);
        let mut two = hits();
        two.push(hit(179.2, Some("[00:01.00]another")));
        assert_eq!(sole_synced_match(two, 180), None);
    }

    #[test]
    fn duet_markers_set_each_lines_voice_and_carry_over() {
        let lrc = "[00:01.00]M: Paper boats along the gutter\n[00:04.00]still floating after rain\n\
                   [00:07.00]F: I folded mine from bus tickets\n[00:10.00]Both: we sail them anyway\n";
        let lines = parse_lrc(lrc, 14_000);
        let voices: Vec<_> = lines.iter().map(|l| l.voice).collect();
        assert_eq!(voices, vec![Some(Voice::M), Some(Voice::M), Some(Voice::F), Some(Voice::Both)]);
        assert_eq!(lines[0].text, "Paper boats along the gutter");
        assert_eq!(lines[0].words[0].text, "Paper");
        let plain = parse_lrc("[00:01.00]Time: half past nine\n", 5_000);
        assert_eq!((plain[0].voice, plain[0].text.as_str()), (None, "Time: half past nine"));
        let stored: Line = serde_json::from_str(r#"{"start_ms":0,"end_ms":1,"text":"x","words":[]}"#).unwrap();
        assert_eq!(stored.voice, None);
        assert_eq!(serde_json::to_value(&lines[3]).unwrap()["voice"], "both");
    }

    #[test]
    #[ignore = "network"]
    fn lrclib_live_lookup_does_not_error() {
        let l = Lrclib::new().unwrap();
        l.fetch("Bohemian Rhapsody", Some("Queen"), None, 355).unwrap();
        l.fetch("zzqxv made up kara song", None, None, 200).unwrap();
    }
}
