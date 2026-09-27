//! Synced lyrics: types, LRC parsing, word timing and lookup.

use crate::fuzzy::{fold, Fuzzy};
use crate::library::LyricsSource;
use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

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
    fn fetch(&self, title: &str, artist: Option<&str>, duration_s: u64) -> Result<Option<String>>;
}

/// lrclib.net — free, no key.
pub struct Lrclib {
    client: reqwest::blocking::Client,
}

impl Lrclib {
    pub fn new() -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("kara-always-oki/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(4))
            .build()?;
        Ok(Self { client })
    }

    /// One search; a busy or unreachable service is a `Transient` error.
    fn search(&self, query: &[(&str, &str)]) -> Result<Vec<Hit>> {
        let transient = |e: reqwest::Error| anyhow::Error::new(e).context(Transient);
        let resp = self.client.get("https://lrclib.net/api/search").query(query).send().map_err(transient)?;
        let status = resp.status();
        if status.is_server_error() || status == reqwest::StatusCode::TOO_MANY_REQUESTS {
            return Err(anyhow::anyhow!(Transient));
        }
        resp.error_for_status()?.json().map_err(transient)
    }
}

impl LyricsFetcher for Lrclib {
    fn fetch(&self, title: &str, artist: Option<&str>, duration_s: u64) -> Result<Option<String>> {
        lookup(|q| self.search(q), title, artist, duration_s)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct Hit {
    track_name: String,
    artist_name: String,
    duration: Option<f64>,
    synced_lyrics: Option<String>,
}

/// A lookup that failed for now (service busy or unreachable) and is worth asking again.
#[derive(Debug)]
struct Transient;

impl std::fmt::Display for Transient {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("the lyrics service didn't answer")
    }
}

/// Pauses before asking a busy service again.
const RETRY_WAITS_MS: [u64; 2] = if cfg!(test) { [1, 1] } else { [700, 2_000] };
/// A busy service is asked again only within this time from the start of a lookup.
const RETRY_WITHIN: Duration = Duration::from_secs(3);
/// A recording further than this from the song's length is another version: by a like singer,
/// with no singer known, and by another singer.
const LIKE_SINGER_OFF_S: f64 = 60.0;
const UNKNOWN_SINGER_OFF_S: f64 = 10.0;
const OTHER_SINGER_OFF_S: f64 = 3.0;

/// Words that mark a video, an upload or a performance rather than the song's name.
const TAGS: &[&str] = &[
    "official", "video", "mv", "audio", "lyrics", "lyric", "hd", "4k", "60fps", "remaster", "remastered", "live",
    "piano only", "instrumental", "inst", "karaoke", "off vocal", "feat", "ft", "cover",
    "现场版", "現場版", "伴奏", "カラオケ", "翻唱", "版本", "官方",
];
/// Words that name a version of a song.
const VERSIONS: &[&str] = &["version", "edit", "mix", "remix", "acoustic", "demo"];
/// Words that name a version only next to a version word or tag, as in "Radio Edit".
const VERSION_MODIFIERS: &[&str] = &["single", "radio", "mono", "stereo"];
/// Endings channels add to a singer's name.
const ARTIST_SUFFIXES: &[&str] = &["- topic", "- 主題", "vevo", "官方", "official"];

/// Runs `f`, asking again after a short pause while the service is busy or unreachable and `deadline` allows.
fn retrying<T>(deadline: Instant, mut f: impl FnMut() -> Result<T>) -> Result<T> {
    for wait in RETRY_WAITS_MS.map(Duration::from_millis) {
        match f() {
            Err(e) if e.is::<Transient>() && Instant::now() + wait < deadline => std::thread::sleep(wait),
            done => return done,
        }
    }
    f()
}

/// Looks the song up by title and singer, then by title alone, and returns the best matching synced lyrics.
fn lookup(search: impl Fn(&[(&str, &str)]) -> Result<Vec<Hit>>, title: &str, artist: Option<&str>, duration_s: u64) -> Result<Option<String>> {
    let deadline = Instant::now() + RETRY_WITHIN;
    let (title, hint) = clean_title(title, artist);
    let artists: Vec<String> = hint.iter().map(String::as_str).chain(artist).flat_map(clean_artist).collect();
    if let Some(a) = artists.first() {
        let hits = retrying(deadline, || search(&[("track_name", &title), ("artist_name", a)]))?;
        if let Some(lyrics) = best_match(hits, &title, &artists, duration_s) {
            return Ok(Some(lyrics));
        }
    }
    Ok(best_match(retrying(deadline, || search(&[("q", &title)]))?, &title, &artists, duration_s))
}

/// The synced lyrics of the hit that best matches the song, scored by title, singer and closeness of length
/// in whole seconds; among equals, the one with more timed lines, then the service's first. Hits without a length are skipped.
/// A hit by a like singer may have a close title and be up to a minute off; by another singer it needs
/// the same title and nearly the same length.
fn best_match(hits: Vec<Hit>, title: &str, artists: &[String], duration_s: u64) -> Option<String> {
    let title_key = key(title);
    hits.into_iter()
        .rev()
        .filter_map(|h| {
            let lyrics = h.synced_lyrics.filter(|s| is_synced(s))?;
            let off = if duration_s == 0 { 0.0 } else { (h.duration? - duration_s as f64).abs() };
            let (their_title, _) = clean_title(&h.track_name, Some(&h.artist_name));
            let same_title = key(&their_title) == title_key;
            let singer = clean_artist(&h.artist_name).iter().flat_map(|a| artists.iter().map(move |b| singer_likeness(a, b))).max().unwrap_or(0);
            let fits = if singer > 0 {
                off <= LIKE_SINGER_OFF_S && (same_title || (similar(title, &their_title) && similar(&their_title, title)))
            } else {
                same_title && off <= if artists.is_empty() { UNKNOWN_SINGER_OFF_S } else { OTHER_SINGER_OFF_S }
            };
            let score = if same_title { 2.0 } else { 1.0 } + f64::from(singer) + 1.0 - off.round() / LIKE_SINGER_OFF_S;
            let timed_lines = lyrics.lines().filter(|l| is_synced(l)).count();
            fits.then_some((score, timed_lines, lyrics))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(.., lyrics)| lyrics)
}

/// How alike two singers' names are: 2 the same; 1 close — a Latin name of 3–4 letters only as a whole word
/// of the other, names under 3 letters never; 0 unlike.
fn singer_likeness(a: &str, b: &str) -> u8 {
    let (ka, kb) = (key(a), key(b));
    let words = |text: &str| words(&fold(&simplified(text)));
    let short = if ka.chars().count() <= kb.chars().count() { &ka } else { &kb };
    let close = match short.chars().count() {
        0..=2 => false,
        3..=4 if short.is_ascii() => words(a).contains(&kb) || words(b).contains(&ka),
        _ => similar(a, b) || similar(b, a),
    };
    if !ka.is_empty() && ka == kb { 2 } else { u8::from(close) }
}

/// Traditional Chinese folded to Simplified, for matching only.
fn simplified(text: &str) -> String {
    fast2s::convert(text)
}

/// Letters and digits only, folded, for telling whether two names are the same.
fn key(text: &str) -> String {
    fold(&simplified(text)).chars().filter(|c| c.is_alphanumeric()).collect()
}

/// Whether `text` holds every word of `query`, forgiving small differences.
fn similar(query: &str, text: &str) -> bool {
    Fuzzy::new(&simplified(query)).and_then(|mut f| f.score(&simplified(text))).is_some()
}

/// The lower-case words of `text`.
fn words(text: &str) -> Vec<String> {
    text.to_lowercase().split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).map(String::from).collect()
}

/// The lower-case words of `text` without its tags (`TAGS`), and whether it had any.
fn untagged(text: &str) -> (Vec<String>, bool) {
    let all = format!(" {} ", words(text).join(" "));
    let mut rest = all.clone();
    for tag in TAGS {
        let tag = if tag.is_ascii() { format!(" {tag} ") } else { tag.to_string() };
        while rest.contains(&tag) {
            rest = rest.replace(&tag, " ");
        }
    }
    (rest.split_whitespace().map(String::from).collect(), rest != all)
}

/// Whether `text` holds a tag or as a whole names a version.
fn noisy(text: &str) -> bool {
    untagged(text).1 || version_phrase(text)
}

/// Whether `text` as a whole names a version, like "Single Version", "2015 Mix", "Remastered 2011" or "Live at …".
fn version_phrase(text: &str) -> bool {
    let all = words(text);
    if all.len() > 2 && all[0] == "live" && ["at", "in", "from", "on"].contains(&all[1].as_str()) {
        return true;
    }
    let (rest, tagged) = untagged(text);
    let is = |list: &[&str], w: &str| list.contains(&w);
    rest.iter().all(|w| w.chars().all(|c| c.is_ascii_digit()) || is(VERSIONS, w) || is(VERSION_MODIFIERS, w))
        && (tagged || rest.iter().any(|w| is(VERSIONS, w)))
}

/// `text` without its bracketed parts — (…) （…） […] 【…】 — and those parts as (opening bracket, contents).
fn brackets(text: &str) -> (String, Vec<(char, &str)>) {
    let (mut outside, mut parts, mut rest) = (String::new(), Vec::new(), text);
    while let Some(i) = rest.find(['(', '（', '[', '【']) {
        let open = rest[i..].chars().next().unwrap_or('(');
        let close = match open { '(' => ')', '（' => '）', '[' => ']', _ => '】' };
        let inner = &rest[i + open.len_utf8()..];
        let Some(j) = inner.find(close) else { break };
        outside.push_str(&rest[..i]);
        outside.push(' ');
        parts.push((open, inner[..j].trim()));
        rest = &inner[j + close.len_utf8()..];
    }
    outside.push_str(rest);
    (outside.split_whitespace().collect::<Vec<_>>().join(" "), parts)
}

/// `text` without `suffix` at its end, ignoring ASCII case.
fn strip_suffix_ci<'a>(text: &'a str, suffix: &str) -> Option<&'a str> {
    let cut = text.len().checked_sub(suffix.len())?;
    (text.is_char_boundary(cut) && text[cut..].eq_ignore_ascii_case(suffix)).then(|| text[..cut].trim_end())
}

/// A song's name as a lyrics service knows it, from a file's or upload's title: without tags like
/// 【4K】, (Official Video) or (Radio Edit), a trailing version like 现场版 and anything after " | "; the name in 《》「」『』 when there is one;
/// for "A - B", A when B is the singer or a version or no singer is known, else B. Also returns a singer named in the title
/// (before 《》「」『』 or " - ").
pub fn clean_title(title: &str, artist: Option<&str>) -> (String, Option<String>) {
    let singer = |s: &str| Some(s.trim_matches(|c: char| c.is_whitespace() || "-|:：".contains(c)).to_string()).filter(|s| !s.is_empty());
    if let Some((before, rest)) = title.split_once(['《', '「', '『']) {
        if let Some((name, after)) = rest.split_once(['》', '」', '』']).filter(|(n, _)| !n.trim().is_empty()) {
            let after = brackets(after).0;
            if after.is_empty() || noisy(&after) {
                return (name.trim().to_string(), singer(&brackets(before).0));
            }
        }
    }
    let (outside, parts) = brackets(title);
    let mut name = outside.split(" | ").next().unwrap_or_default().to_string();
    for (open, part) in parts {
        if matches!(open, '(' | '（') && !noisy(part) {
            name = format!("{name} ({part})");
        }
    }
    let is_singer = |part: &str| artist.into_iter().flat_map(clean_artist).any(|a| key(&a) == key(part));
    let (name, named) = match name.split_once(" - ") {
        Some((left, right)) if is_singer(right) || version_phrase(right) || artist.is_none() => (left, None),
        Some((left, right)) => (right, singer(left).filter(|_| !is_singer(left))),
        None => (name.as_str(), None),
    };
    let mut name = name.trim();
    while let Some(rest) = TAGS.iter().filter(|n| !n.is_ascii()).find_map(|n| name.strip_suffix(n)).filter(|r| !r.trim().is_empty()) {
        name = rest.trim_end();
    }
    if name.is_empty() { (title.trim().to_string(), None) } else { (name.to_string(), named) }
}

/// A singer's names as a lyrics service knows them: without endings like " - Topic" or "VEVO" and
/// bracketed version words; a name in brackets is another name for the same singer.
pub fn clean_artist(artist: &str) -> Vec<String> {
    let (outside, parts) = brackets(artist);
    let mut name = outside.as_str();
    while let Some(rest) = ARTIST_SUFFIXES.iter().find_map(|s| strip_suffix_ci(name, s)).filter(|r| !r.is_empty()) {
        name = rest;
    }
    std::iter::once(name).chain(parts.into_iter().map(|(_, p)| p).filter(|p| !noisy(p))).filter(|n| !n.is_empty()).map(String::from).collect()
}

/// Embedded synced lyrics first, then the online service, else none.
pub fn find(embedded: Option<&str>, title: &str, artist: Option<&str>, duration_ms: i64, fetcher: &dyn LyricsFetcher) -> Result<(LyricsSource, Vec<Line>)> {
    if let Some(text) = embedded.filter(|t| is_synced(t)) {
        return Ok((LyricsSource::Embedded, parse_lrc(text, duration_ms)));
    }
    if let Some(text) = fetcher.fetch(title, artist, (duration_ms / 1000) as u64)? {
        return Ok((LyricsSource::Lrclib, parse_lrc(&text, duration_ms)));
    }
    Ok((LyricsSource::None, Vec::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::{Cell, RefCell};

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
        fn fetch(&self, _: &str, _: Option<&str>, _: u64) -> Result<Option<String>> {
            *self.1.borrow_mut() += 1;
            Ok(self.0.map(String::from))
        }
    }

    #[test]
    fn find_prefers_embedded_then_fetches_then_gives_up() {
        let f = Fake(Some("[00:01.00]online"), RefCell::new(0));
        let (src, lines) = find(Some("[00:00.00]embedded"), "t", Some("a"), 5_000, &f).unwrap();
        assert_eq!((src, lines[0].text.as_str(), *f.1.borrow()), (LyricsSource::Embedded, "embedded", 0));
        let (src, lines) = find(Some("plain unsynced"), "t", Some("a"), 5_000, &f).unwrap();
        assert_eq!((src, lines[0].text.as_str()), (LyricsSource::Lrclib, "online"));
        let none = Fake(None, RefCell::new(0));
        assert_eq!(find(None, "t", Some("a"), 5_000, &none).unwrap(), (LyricsSource::None, vec![]));
        // No artist: still asks, by title.
        assert_eq!(find(None, "t", None, 5_000, &f).unwrap().0, LyricsSource::Lrclib);
        // A leading UTF-8 BOM doesn't stop embedded text from being recognized as synced.
        let (src, lines) = find(Some("\u{FEFF}[00:00.00]bommed"), "t", Some("a"), 5_000, &f).unwrap();
        assert_eq!((src, lines[0].text.as_str()), (LyricsSource::Embedded, "bommed"));
    }

    #[test]
    fn cleans_titles_and_artists_as_files_and_uploads_give_them() {
        let title = clean_title;
        assert_eq!(title("Long Long Time | Piano Only", None), ("Long Long Time".into(), None));
        assert_eq!(title("太陽與地球 伴奏", Some("盧廣仲(版本)")), ("太陽與地球".into(), None));
        assert_eq!(title("【4K60FPS】张学友《李香兰》现场版", None), ("李香兰".into(), Some("张学友".into())));
        assert_eq!(title("Juniper Row - Paper Boats (Official Video)", Some("JuniperRowVEVO")), ("Paper Boats".into(), None));
        assert_eq!(title("Paper Boats - Remastered 2011", Some("Juniper Row")), ("Paper Boats".into(), None));
        assert_eq!(title("Paper Boats (Part 2) [HD]", None), ("Paper Boats (Part 2)".into(), None));
        assert_eq!(title("Juniper Row - Paper Boats", Some("Some Uploader")), ("Paper Boats".into(), Some("Juniper Row".into())));
        assert_eq!(title("Paper Boats - Juniper Row", Some("Juniper Row")), ("Paper Boats".into(), None));
        assert_eq!(title("张学友 - 《李香兰》", None), ("李香兰".into(), Some("张学友".into())));
        assert_eq!(title("「さよなら」の意味", None), ("「さよなら」の意味".into(), None));
        assert_eq!(title("Stand By Me - Single Version", Some("Ben E. King")), ("Stand By Me".into(), None));
        assert_eq!(title("Jolene - Acoustic", Some("Dolly Parton")), ("Jolene".into(), None));
        assert_eq!(title("Hey Jude - 2015 Mix", Some("The Beatles")), ("Hey Jude".into(), None));
        assert_eq!(title("Hello - Adele", None), ("Hello".into(), None));
        assert_eq!(title("Adele - Hello", Some("Adele")), ("Hello".into(), None));
        assert_eq!(title("Beyoncé - Single Ladies (Put a Ring on It)", Some("Beyoncé")), ("Single Ladies (Put a Ring on It)".into(), None));
        assert_eq!(title("Radio - Single Version", Some("Lana Del Rey")), ("Radio".into(), None));
        assert_eq!(title("Lana Del Rey - Radio", Some("Lana Del Rey")), ("Radio".into(), None));
        assert_eq!(title("Hozier - From Eden", Some("Hozier")), ("From Eden".into(), None));
        assert_eq!(title("Paper Boats - Live at Wembley", Some("Juniper Row")), ("Paper Boats".into(), None));
        assert_eq!(title("Paper Boats (From Me to You) (Radio Edit)", None), ("Paper Boats (From Me to You)".into(), None));
        assert_eq!(clean_artist("盧廣仲(版本)"), ["盧廣仲"]);
        assert_eq!(clean_artist("盧廣仲 (Crowd Lu)"), ["盧廣仲", "Crowd Lu"]);
        assert_eq!(clean_artist("Crowd Lu - Topic"), ["Crowd Lu"]);
        assert_eq!(clean_artist("盧廣仲 - 主題"), ["盧廣仲"]);
        assert_eq!(clean_artist("JuniperRowVEVO"), ["JuniperRow"]);
    }

    fn hit(track: &str, artist: &str, duration: f64, lyrics: Option<&str>) -> Hit {
        Hit { track_name: track.into(), artist_name: artist.into(), duration: Some(duration), synced_lyrics: lyrics.map(String::from) }
    }

    #[test]
    fn best_match_prefers_the_singer_then_the_closest_length_and_skips_weak_ones() {
        let lin = vec!["林小雨".to_string()];
        let night = vec![
            hit("夜車", "Someone Else", 240.0, Some("[00:01.00]other singer")),
            hit("夜车", "Lin Xiaoyu (林小雨)", 250.0, Some("[00:01.00]hers")),
            hit("夜車", "林小雨", 240.0, Some("not synced")),
        ];
        assert_eq!(best_match(night, "夜車", &lin, 240).as_deref(), Some("[00:01.00]hers"));

        let boats = || {
            vec![
                hit("Paper Boats", "A", 290.0, Some("[00:01.00]far")),
                hit("Paper Boats (Live)", "B", 230.0, Some("[00:01.00]near")),
                hit("Paper Boats", "C", 215.0, Some("[00:01.00]nearest")),
            ]
        };
        assert_eq!(best_match(boats(), "Paper Boats", &[], 208).as_deref(), Some("[00:01.00]nearest"));
        assert_eq!(best_match(boats(), "Paper Boats", &[], 200), None);
        assert_eq!(best_match(boats(), "Paper Boats", &["Nobody Real".into()], 213).as_deref(), Some("[00:01.00]nearest"));
        assert_eq!(best_match(boats(), "Paper Boats", &["Nobody Real".into()], 208), None);

        let stay = vec![hit("Stay - Big Star", "Big Star", 239.0, Some("[00:01.00]theirs")), hit("STAY", "Home Crew", 205.0, Some("[00:01.00]other"))];
        assert_eq!(best_match(stay, "Stay", &["Nobody Real".into()], 200), None);

        let close_title = || vec![hit("Paper Boat", "Juniper Row & Friends", 230.0, Some("[00:01.00]close title"))];
        assert_eq!(best_match(close_title(), "Paper Boats", &[], 200), None);
        assert_eq!(best_match(close_title(), "Paper Boats", &["Juniper Row".into()], 200).as_deref(), Some("[00:01.00]close title"));
        assert_eq!(best_match(vec![hit("Paper Boats", "G-Star", 220.0, Some("[00:01.00]g"))], "Paper Boats", &["G".into()], 200), None);

        let ties = vec![
            hit("Paper Boats", "Juniper Row", 200.4, Some("[00:01.00]short")),
            hit("Paper Boats", "Juniper Row", 200.0, Some("[00:01.00]long\n[00:02.00]er")),
            hit("Paper Boats", "Juniper Row", 200.0, Some("[00:01.00]late\n[00:02.00]copy")),
        ];
        assert_eq!(best_match(ties, "Paper Boats", &["Juniper Row".into()], 200).as_deref(), Some("[00:01.00]long\n[00:02.00]er"));
        assert_eq!(singer_likeness("Bob", "Bob Dylan"), 1);
        assert_eq!((singer_likeness("Bob", "Nobody Real Band"), singer_likeness("Eve", "Evanescence")), (0, 0));
        assert_eq!(singer_likeness("林小雨", "林小雨與朋友"), 1);

        let no_length: Vec<Hit> = serde_json::from_str(r#"[{"trackName":"Paper Boats","artistName":"Juniper Row","duration":null,"syncedLyrics":"[00:01.00]x"}]"#).unwrap();
        assert_eq!(best_match(no_length, "Paper Boats", &["Juniper Row".into()], 200), None);

        let other_script = vec![hit("夜車", "Lin Xiaoyu - Topic", 240.0, Some("[00:01.00]same length")), hit("夜車", "Someone Else", 262.0, Some("[00:01.00]other length"))];
        assert_eq!(best_match(other_script, "夜車", &lin, 240).as_deref(), Some("[00:01.00]same length"));
    }

    #[test]
    fn a_busy_service_is_asked_again_and_an_outage_finds_nothing_to_store() {
        let calls = Cell::new(0);
        let flaky = |_: &[(&str, &str)]| {
            calls.set(calls.get() + 1);
            match calls.get() {
                1 => Err(anyhow::anyhow!(Transient)),
                _ => Ok(vec![hit("Paper Boats", "Juniper Row", 200.0, Some("[00:01.00]found"))]),
            }
        };
        assert_eq!(lookup(flaky, "Paper Boats", None, 200).unwrap().as_deref(), Some("[00:01.00]found"));
        let calls = Cell::new(0);
        let down = |_: &[(&str, &str)]| -> Result<Vec<Hit>> {
            calls.set(calls.get() + 1);
            Err(anyhow::anyhow!(Transient))
        };
        assert!(lookup(down, "Paper Boats", None, 200).is_err());
        assert_eq!(calls.get(), 3);
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
    fn lrclib_live_lookup_finds_messy_titles() {
        let l = Lrclib::new().unwrap();
        for (title, artist, duration_s) in [
            ("Long Long Time | Piano Only", None, 262),
            ("太陽與地球 伴奏", Some("盧廣仲(版本)"), 262),
            ("【4K60FPS】张学友《李香兰》现场版", None, 397),
        ] {
            let lines = l.fetch(title, artist, duration_s).unwrap().map(|t| parse_lrc(&t, 0).len());
            println!("{title}: {lines:?} lines");
            assert!(lines.is_some_and(|n| n > 0), "{title}");
        }
        assert_eq!(l.fetch("zzqxv made up kara song", None, 200).unwrap(), None);
    }
}
