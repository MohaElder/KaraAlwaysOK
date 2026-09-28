//! Synced lyrics: types, LRC parsing, word timing and lookup.

use crate::fuzzy::{fold, Fuzzy};
use crate::ingest::ytdlp::clean_meta;
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

/// Synced lyrics, with the song's title and singer as the lyrics service names them.
#[derive(Clone, Debug, PartialEq)]
pub struct Found {
    pub lyrics: String,
    pub title: String,
    pub artist: String,
}

/// Made-up lyrics with no names, for tests.
#[cfg(test)]
impl From<&str> for Found {
    fn from(lyrics: &str) -> Self {
        Found { lyrics: lyrics.into(), title: String::new(), artist: String::new() }
    }
}

/// A source of synced (LRC) lyrics for a song.
pub trait LyricsFetcher {
    /// Without an artist, matches by title and duration.
    fn fetch(&self, title: &str, artist: Option<&str>, duration_s: u64) -> Result<Option<Found>>;

    /// The first of `guesses` (title, singer) that a music catalog lists near this length; None when none is or it can't tell.
    fn pick_name(&self, _guesses: &[(String, String)], _duration_s: u64) -> Option<(String, String)> {
        None
    }
}

/// lrclib.net — free, no key — with MusicBrainz for song names it doesn't know.
pub struct Lrclib {
    client: reqwest::blocking::Client,
}

impl Lrclib {
    pub fn new() -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(USER_AGENT)
            .timeout(Duration::from_secs(8))
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

    /// MusicBrainz recordings named like `title` by a singer named like `artist`, one request per `MUSICBRAINZ_PACE`;
    /// none while it is busy.
    fn recordings(&self, title: &str, artist: &str) -> Result<Vec<Hit>> {
        let quoted = |s: &str| format!("\"{}\"", s.replace('\\', "\\\\").replace('"', "\\\""));
        let query = format!("recording:{} AND artist:{}", quoted(title), quoted(artist));
        let mut last = MUSICBRAINZ_LAST.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(at) = *last {
            std::thread::sleep(MUSICBRAINZ_PACE.saturating_sub(at.elapsed()));
        }
        let resp = self.client.get("https://musicbrainz.org/ws/2/recording").query(&[("query", query.as_str()), ("fmt", "json"), ("limit", "10")]).send();
        *last = Some(Instant::now());
        let resp = resp?;
        if resp.status() == reqwest::StatusCode::SERVICE_UNAVAILABLE {
            return Ok(Vec::new());
        }
        let found: Recordings = resp.error_for_status()?.json()?;
        Ok(found.recordings.into_iter().map(Hit::from).collect())
    }
}

impl LyricsFetcher for Lrclib {
    fn fetch(&self, title: &str, artist: Option<&str>, duration_s: u64) -> Result<Option<Found>> {
        lookup(|q| self.search(q), title, artist, duration_s)
    }

    /// Asks LRCLIB first, then MusicBrainz.
    fn pick_name(&self, guesses: &[(String, String)], duration_s: u64) -> Option<(String, String)> {
        let lrclib = |title: &str, artist: &str| self.search(&[("track_name", title), ("artist_name", artist)]);
        pick(guesses, duration_s, lrclib).ok().flatten().or_else(|| pick(guesses, duration_s, |t, a| self.recordings(t, a)).ok().flatten())
    }
}

/// User-Agent for LRCLIB and MusicBrainz requests.
const USER_AGENT: &str = concat!("kara-always-oki/", env!("CARGO_PKG_VERSION"), " ( https://github.com/GITHUB-OWNER/kara-always-oki )");
/// Time between MusicBrainz requests.
const MUSICBRAINZ_PACE: Duration = Duration::from_millis(1_100);
/// When MusicBrainz was last asked.
static MUSICBRAINZ_LAST: std::sync::Mutex<Option<Instant>> = std::sync::Mutex::new(None);

#[derive(Deserialize)]
struct Recordings {
    recordings: Vec<Recording>,
}

#[derive(Deserialize)]
struct Recording {
    title: String,
    length: Option<f64>,
    #[serde(rename = "artist-credit", default)]
    artist_credit: Vec<Credit>,
}

#[derive(Deserialize)]
struct Credit {
    name: String,
    #[serde(default)]
    joinphrase: String,
}

impl From<Recording> for Hit {
    fn from(r: Recording) -> Self {
        let artist_name = r.artist_credit.iter().map(|c| format!("{}{}", c.name, c.joinphrase)).collect();
        Hit { track_name: r.title, artist_name, duration: r.length.map(|ms| ms / 1000.0), synced_lyrics: None }
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
const ARTIST_SUFFIXES: &[&str] = &["- topic", "- 主題", "vevo", "官方频道", "官方頻道", "官方", "official"];

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
fn lookup(search: impl Fn(&[(&str, &str)]) -> Result<Vec<Hit>>, title: &str, artist: Option<&str>, duration_s: u64) -> Result<Option<Found>> {
    let deadline = Instant::now() + RETRY_WITHIN;
    let (title, hint) = clean_title(title, artist);
    let artists: Vec<String> = hint.iter().map(String::as_str).chain(artist).flat_map(clean_artist).collect();
    if let Some(a) = artists.first() {
        let hits = retrying(deadline, || search(&[("track_name", &title), ("artist_name", a)]))?;
        if let Some(lyrics) = best_lyrics(hits, &title, &artists, duration_s) {
            return Ok(Some(lyrics));
        }
    }
    Ok(best_lyrics(retrying(deadline, || search(&[("q", &title)]))?, &title, &artists, duration_s))
}

/// The hit that best matches the song, scored by title, singer and closeness of length
/// in whole seconds; among equals, the one with more timed lines, then the service's first. Hits without a length are skipped.
/// A hit by a like singer may have a close title and be up to a minute off; by another singer it needs
/// the same title and nearly the same length.
fn best_match(hits: Vec<Hit>, title: &str, artists: &[String], duration_s: u64) -> Option<Hit> {
    let title_key = key(title);
    hits.into_iter()
        .rev()
        .filter_map(|h| {
            let off = if duration_s == 0 { 0.0 } else { (h.duration? - duration_s as f64).abs() };
            let (their_title, _) = clean_title(&h.track_name, Some(&h.artist_name));
            let same_title = key(&their_title) == title_key;
            let singer = singer_of(&h.artist_name, artists);
            let fits = if singer > 0 {
                off <= LIKE_SINGER_OFF_S && (same_title || (similar(title, &their_title) && similar(&their_title, title)))
            } else {
                same_title && off <= if artists.is_empty() { UNKNOWN_SINGER_OFF_S } else { OTHER_SINGER_OFF_S }
            };
            let score = if same_title { 2.0 } else { 1.0 } + f64::from(singer) + 1.0 - off.round() / LIKE_SINGER_OFF_S;
            let timed_lines = h.synced_lyrics.as_deref().unwrap_or_default().lines().filter(|l| is_synced(l)).count();
            fits.then_some((score, timed_lines, h))
        })
        .max_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)))
        .map(|(.., hit)| hit)
}

/// The synced lyrics and names of the hit with synced lyrics that best matches the song (see `best_match`).
fn best_lyrics(hits: Vec<Hit>, title: &str, artists: &[String], duration_s: u64) -> Option<Found> {
    let synced = hits.into_iter().filter(|h| h.synced_lyrics.as_deref().is_some_and(is_synced)).collect();
    best_match(synced, title, artists, duration_s).map(|h| Found { lyrics: h.synced_lyrics.unwrap_or_default(), title: h.track_name, artist: h.artist_name })
}

/// How alike a hit's singer is to the closest of `artists` (see `singer_likeness`).
fn singer_of(their_artist: &str, artists: &[String]) -> u8 {
    clean_artist(their_artist).iter().flat_map(|a| artists.iter().map(move |b| singer_likeness(a, b))).max().unwrap_or(0)
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
/// for "A - B", B when A is the singer, A when B is the singer, a version, starts with "from" or no singer is known, else B. Also returns a singer named in the title
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
        Some((left, right)) if !is_singer(left) && (is_singer(right) || version_phrase(right) || words(right).first().is_some_and(|w| w == "from") || artist.is_none()) => {
            (left, None)
        }
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

/// Marks that split a video title into names.
const SEPARATORS: &[&str] = &[" - ", " / ", "–", "—", "|", "：", "《", "》", "「", "」", "『", "』", "“", "”"];
/// Marks that split a video title into names except inside a Latin word or number, as in "Anti-Hero" or "AB/CD".
const WORD_SEPARATORS: [char; 3] = ['-', '/', ':'];
/// At most this many name guesses are checked per song.
const MAX_GUESSES: usize = 6;

/// `name` without a featured singer (" feat. …", " ft. …", " featuring …").
fn before_feat(name: &str) -> &str {
    let lower = name.to_ascii_lowercase();
    let cut = [" feat. ", " feat ", " ft. ", " featuring "].iter().filter_map(|m| lower.find(m)).min();
    cut.map_or(name, |i| &name[..i]).trim()
}

/// Likely (title, singer) pairs for a video, most likely first: today's names (`clean_meta`), then the title's parts
/// between separators paired both ways, then each part with the channel.
pub fn name_guesses(title: &str, channel: Option<&str>) -> Vec<(String, String)> {
    let mut pairs = vec![clean_meta(title, channel)];
    let channel = channel.and_then(|c| clean_artist(c).into_iter().next());
    let mut text = brackets(title).0;
    for s in SEPARATORS {
        text = text.replace(s, "\n");
    }
    let chars: Vec<char> = text.chars().collect();
    let latin = |i: Option<usize>| i.and_then(|i| chars.get(i)).is_some_and(char::is_ascii_alphanumeric);
    let text: String = chars.iter().enumerate().map(|(i, &c)| if WORD_SEPARATORS.contains(&c) && !(latin(i.checked_sub(1)) && latin(Some(i + 1))) { '\n' } else { c }).collect();
    let parts: Vec<String> = text.split('\n').map(before_feat).filter(|p| !p.is_empty() && !noisy(p)).map(|p| clean_title(p, None).0).collect();
    for (i, a) in parts.iter().enumerate() {
        for b in &parts[i + 1..] {
            pairs.push((b.clone(), Some(a.clone())));
            pairs.push((a.clone(), Some(b.clone())));
        }
    }
    pairs.extend(parts.iter().map(|p| (p.clone(), channel.clone())));
    let mut seen = std::collections::HashSet::new();
    pairs
        .into_iter()
        .filter_map(|(t, a)| Some((t, before_feat(&a?).to_string())))
        .filter(|(t, a)| !key(t).is_empty() && !key(a).is_empty() && key(t) != key(a) && seen.insert((key(t), key(a))))
        .take(MAX_GUESSES)
        .collect()
}

/// The first guess whose `search` finds a recording by a like singer, with a like title, near the song's length (see `best_match`);
/// swapped when the swapped guess is one too and more recordings match it.
fn pick(guesses: &[(String, String)], duration_s: u64, search: impl Fn(&str, &str) -> Result<Vec<Hit>>) -> Result<Option<(String, String)>> {
    let matches = |title: &str, artist: &str| -> Result<usize> {
        let artists = clean_artist(artist);
        let hits = search(title, artist)?.into_iter().filter_map(|h| best_match(vec![h], title, &artists, duration_s));
        Ok(hits.filter(|h| singer_of(&h.artist_name, &artists) > 0).count())
    };
    for (title, artist) in guesses {
        let n = matches(title, artist)?;
        if n > 0 {
            let swapped = guesses.iter().any(|(t, a)| t == artist && a == title) && matches(artist, title)? > n;
            return Ok(Some(if swapped { (artist.clone(), title.clone()) } else { (title.clone(), artist.clone()) }));
        }
    }
    Ok(None)
}

/// A lyrics service that answers every search with the same made-up (title, singer, length in seconds) candidates.
#[cfg(test)]
pub(crate) struct Candidates(pub Vec<(&'static str, &'static str, f64)>);

#[cfg(test)]
impl LyricsFetcher for Candidates {
    fn fetch(&self, title: &str, artist: Option<&str>, duration_s: u64) -> Result<Option<Found>> {
        let hits = |_: &[(&str, &str)]| {
            Ok(self.0.iter().map(|&(t, a, d)| Hit { track_name: t.into(), artist_name: a.into(), duration: Some(d), synced_lyrics: Some("[00:01.00]la la la".into()) }).collect())
        };
        lookup(hits, title, artist, duration_s)
    }
}

/// Embedded synced lyrics first, then the online service, then the service with title and singer swapped, else none.
/// With the swap, also returns what the service found, which names the song.
pub fn find(embedded: Option<&str>, title: &str, artist: Option<&str>, duration_ms: i64, fetcher: &dyn LyricsFetcher) -> Result<(LyricsSource, Vec<Line>, Option<Found>)> {
    if let Some(text) = embedded.filter(|t| is_synced(t)) {
        return Ok((LyricsSource::Embedded, parse_lrc(text, duration_ms), None));
    }
    let duration_s = (duration_ms / 1000) as u64;
    if let Some(f) = fetcher.fetch(title, artist, duration_s)? {
        return Ok((LyricsSource::Lrclib, parse_lrc(&f.lyrics, duration_ms), None));
    }
    if let Some(f) = artist.map(|a| fetcher.fetch(a, Some(title), duration_s)).transpose()?.flatten() {
        return Ok((LyricsSource::Lrclib, parse_lrc(&f.lyrics, duration_ms), Some(f)));
    }
    Ok((LyricsSource::None, Vec::new(), None))
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
        fn fetch(&self, _: &str, _: Option<&str>, _: u64) -> Result<Option<Found>> {
            *self.1.borrow_mut() += 1;
            Ok(self.0.map(Found::from))
        }
    }

    #[test]
    fn find_prefers_embedded_then_fetches_then_gives_up() {
        let f = Fake(Some("[00:01.00]online"), RefCell::new(0));
        let (src, lines, _) = find(Some("[00:00.00]embedded"), "t", Some("a"), 5_000, &f).unwrap();
        assert_eq!((src, lines[0].text.as_str(), *f.1.borrow()), (LyricsSource::Embedded, "embedded", 0));
        let (src, lines, _) = find(Some("plain unsynced"), "t", Some("a"), 5_000, &f).unwrap();
        assert_eq!((src, lines[0].text.as_str()), (LyricsSource::Lrclib, "online"));
        let none = Fake(None, RefCell::new(0));
        assert_eq!(find(None, "t", Some("a"), 5_000, &none).unwrap(), (LyricsSource::None, vec![], None));
        // No artist: still asks, by title.
        assert_eq!(find(None, "t", None, 5_000, &f).unwrap().0, LyricsSource::Lrclib);
        // A leading UTF-8 BOM doesn't stop embedded text from being recognized as synced.
        let (src, lines, _) = find(Some("\u{FEFF}[00:00.00]bommed"), "t", Some("a"), 5_000, &f).unwrap();
        assert_eq!((src, lines[0].text.as_str()), (LyricsSource::Embedded, "bommed"));
    }

    #[test]
    fn a_song_found_only_with_title_and_singer_swapped_gets_the_services_names() {
        let stars = Candidates(vec![("City of Stars", "Ryan Gosling & Emma Stone", 150.0)]);
        let (title, artist) = ("La La Land Original Motion Picture Soundtrack", Some("'City of Stars' (Duet ft. Ryan Gosling, Emma Stone)"));
        let (src, _, renamed) = find(None, title, artist, 150_000, &stars).unwrap();
        let renamed = renamed.map(|f| (f.title, f.artist));
        assert_eq!((src, renamed), (LyricsSource::Lrclib, Some(("City of Stars".into(), "Ryan Gosling & Emma Stone".into()))));
        let (src, _, renamed) = find(None, "City of Stars", Some("Ryan Gosling"), 150_000, &stars).unwrap();
        assert_eq!((src, renamed), (LyricsSource::Lrclib, None));
        let (src, _, renamed) = find(None, title, artist, 200_000, &stars).unwrap();
        assert_eq!((src, renamed), (LyricsSource::None, None));
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
        assert_eq!(title("Let It Go - From \"Frozen\"/Soundtrack Version", Some("Idina Menzel")), ("Let It Go".into(), None));
        assert_eq!(title("India.Arie - Video", Some("India.Arie")), ("Video".into(), None));
        assert_eq!(title("Portugal. The Man - Live in the Moment", Some("Portugal. The Man")), ("Live in the Moment".into(), None));
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
        let best_match = |hits, title, artists: &[String], duration_s| best_lyrics(hits, title, artists, duration_s).map(|f| f.lyrics);
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
    fn guesses_names_from_a_video_title_and_its_channel_starting_with_todays() {
        let pairs = |title, channel| name_guesses(title, channel).into_iter().map(|(t, a)| format!("{t} / {a}")).collect::<Vec<_>>();
        assert_eq!(
            pairs("【4K60FPS】林小雨《夜车》经典现场！好听到哭", Some("Made Up Uploader")),
            [
                "【4K60FPS】林小雨《夜车》经典现场！好听到哭 / Made Up Uploader",
                "夜车 / 林小雨",
                "林小雨 / 夜车",
                "经典现场！好听到哭 / 林小雨",
                "林小雨 / 经典现场！好听到哭",
                "经典现场！好听到哭 / 夜车",
            ]
        );
        assert_eq!(pairs("Juniper Row feat. Kiko - Paper Boats (Official Video)", Some("JuniperRowVEVO")), ["Paper Boats / Juniper Row", "Juniper Row / Paper Boats"]);
        assert_eq!(pairs("Kiko-Ray - Paper-Boats (Official Video)", Some("KikoRayVEVO")), ["Paper-Boats / Kiko-Ray", "Kiko-Ray / Paper-Boats"]);
        assert_eq!(pairs("AB/CD - Paper Boats", Some("abcdVEVO")), ["Paper Boats / AB/CD", "AB/CD / Paper Boats"]);
        assert!(pairs("【71】夜车（林小雨2003巡回live演唱会）— 林小雨 【动态鼓谱】", None).contains(&"夜车 / 林小雨".to_string()));
        assert_eq!(
            pairs("林小雨-夜车", Some("夜车粉丝官方频道（虫）")),
            ["林小雨-夜车 / 夜车粉丝官方频道（虫）", "夜车 / 林小雨", "林小雨 / 夜车", "林小雨 / 夜车粉丝", "夜车 / 夜车粉丝"]
        );
        assert!(pairs("Paper Boats", None).is_empty());
    }

    #[test]
    fn picks_the_first_guess_a_catalog_lists_by_a_like_singer_near_the_songs_length() {
        let guesses = |v: &[(&str, &str)]| v.iter().map(|&(t, a)| (t.to_string(), a.to_string())).collect::<Vec<_>>();
        let catalog = |title: &str, _: &str| -> Result<Vec<Hit>> {
            Ok(match title {
                "夜车" => vec![hit("夜車", "林小雨", 240.0, None)],
                "Paper Boats" => vec![hit("Paper Boats", "Someone Else", 240.0, None)],
                _ => vec![],
            })
        };
        let named = pick(&guesses(&[("林小雨", "夜车"), ("夜车", "林小雨")]), 250, catalog).unwrap();
        assert_eq!(named, Some(("夜车".into(), "林小雨".into())));
        assert_eq!(pick(&guesses(&[("夜车", "林小雨")]), 400, catalog).unwrap(), None);
        assert_eq!(pick(&guesses(&[("Paper Boats", "Juniper Row")]), 240, catalog).unwrap(), None);
        assert!(pick(&guesses(&[("夜车", "林小雨")]), 240, |_, _| Err(anyhow::anyhow!("offline"))).is_err());
        let swapped_too = |title: &str, _: &str| -> Result<Vec<Hit>> {
            Ok(match title {
                "Juniper Row" => vec![hit("Juniper Row", "Paper Boats", 240.0, None)],
                _ => vec![hit("Paper Boats", "Juniper Row", 240.0, None), hit("Paper Boats", "Juniper Row", 241.0, None)],
            })
        };
        let both_ways = guesses(&[("Juniper Row", "Paper Boats"), ("Paper Boats", "Juniper Row")]);
        assert_eq!(pick(&both_ways, 240, swapped_too).unwrap(), Some(("Paper Boats".into(), "Juniper Row".into())));

        let json = r#"{"recordings":[{"title":"Paper Boats","length":241000,"artist-credit":[{"name":"Juniper Row","joinphrase":" & "},{"name":"Kiko"}]},
                                     {"title":"Paper Boats","artist-credit":[{"name":"Juniper Row"}]}]}"#;
        let musicbrainz = |_: &str, _: &str| -> Result<Vec<Hit>> { Ok(serde_json::from_str::<Recordings>(json)?.recordings.into_iter().map(Hit::from).collect()) };
        let hits = musicbrainz("", "").unwrap();
        assert_eq!((hits[0].artist_name.as_str(), hits[0].duration, hits[1].duration), ("Juniper Row & Kiko", Some(241.0), None));
        assert_eq!(pick(&guesses(&[("Paper Boats", "Juniper Row")]), 250, musicbrainz).unwrap(), Some(("Paper Boats".into(), "Juniper Row".into())));
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
        assert_eq!(lookup(flaky, "Paper Boats", None, 200).unwrap().map(|f| f.lyrics).as_deref(), Some("[00:01.00]found"));
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
            let lines = l.fetch(title, artist, duration_s).unwrap().map(|f| parse_lrc(&f.lyrics, 0).len());
            println!("{title}: {lines:?} lines");
            assert!(lines.is_some_and(|n| n > 0), "{title}");
        }
        assert_eq!(l.fetch("zzqxv made up kara song", None, 200).unwrap(), None);
        let (title, artist) = ("La La Land Original Motion Picture Soundtrack", Some("'City of Stars' (Duet ft. Ryan Gosling, Emma Stone)"));
        let (src, lines, renamed) = find(None, title, artist, 150_000, &l).unwrap();
        let renamed = renamed.map(|f| (f.title, f.artist));
        println!("swapped: {src:?}, {} lines, now {renamed:?}", lines.len());
        assert!(renamed.is_some());
    }

    #[test]
    #[ignore = "network"]
    fn live_names_come_from_lrclib_or_else_musicbrainz() {
        let l = Lrclib::new().unwrap();
        let li = Some(("李香兰".to_string(), "张学友".to_string()));
        assert_eq!(l.pick_name(&name_guesses("张学友-李香兰", Some("叶斐（虫）")), 441), li);
        assert_eq!(l.pick_name(&name_guesses("【4K60FPS】张学友《李香兰》经典神级现场！好歌如酒如痴如醉", Some("音乐私藏馆")), 462), li);
    }
}
