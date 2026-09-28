//! YouTube's own web search and search suggestions, called directly.

use super::preview::{LinkPreview, SearchHit, SEARCH_RESULTS, SUGGESTIONS};
use anyhow::Result;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::OnceLock;
use std::time::Duration;

/// The web client youtube.com identifies as (yt-dlp's `INNERTUBE_CLIENTS["web"]`).
const CLIENT_VERSION: &str = "2.20260708.00.00";
/// The search filter for videos only (yt-dlp's `YoutubeSearchIE._SEARCH_PARAMS`).
const VIDEOS_ONLY: &str = "EgIQAfABAQ==";

/// One client for every call, so later ones reuse the open connection.
fn client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(reqwest::blocking::Client::new)
}

/// The top YouTube videos for `query`, from the search youtube.com itself uses.
pub fn search(query: &str) -> Result<Vec<SearchHit>> {
    let body = json!({ "context": { "client": { "clientName": "WEB", "clientVersion": CLIENT_VERSION } }, "query": query, "params": VIDEOS_ONLY });
    let text = client()
        .post("https://www.youtube.com/youtubei/v1/search?prettyPrint=false")
        .timeout(Duration::from_secs(5))
        .header("X-YouTube-Client-Name", "1")
        .header("X-YouTube-Client-Version", CLIENT_VERSION)
        .header("Origin", "https://www.youtube.com")
        .json(&body)
        .send()?
        .error_for_status()?
        .text()?;
    let mut hits = parse_search(&text)?;
    hits.truncate(SEARCH_RESULTS);
    Ok(hits)
}

/// The videos in a YouTube web search response, once each and none live or still to come.
pub fn parse_search(json: &str) -> Result<Vec<SearchHit>> {
    let response: Value = serde_json::from_str(json)?;
    let mut videos = Vec::new();
    collect_videos(&response, &mut videos);
    let mut seen = HashSet::new();
    Ok(videos.into_iter().filter(|v| !is_live_or_upcoming(v)).filter_map(hit).filter(|h| seen.insert(h.url.clone())).collect())
}

fn collect_videos<'a>(v: &'a Value, out: &mut Vec<&'a Value>) {
    match v {
        Value::Object(m) => match m.get("videoRenderer") {
            Some(video) => out.push(video),
            None => m.values().for_each(|x| collect_videos(x, out)),
        },
        Value::Array(a) => a.iter().for_each(|x| collect_videos(x, out)),
        _ => {}
    }
}

fn is_live_or_upcoming(video: &Value) -> bool {
    let live_badge = video["badges"].as_array().is_some_and(|b| b.iter().any(|b| b["metadataBadgeRenderer"]["style"] == "BADGE_STYLE_TYPE_LIVE_NOW"));
    live_badge || video.get("upcomingEventData").is_some()
}

fn text(v: &Value) -> Option<String> {
    v["simpleText"].as_str().map(Into::into).or_else(|| Some(v["runs"].as_array()?.iter().filter_map(|r| r["text"].as_str()).collect()))
}

fn hit(video: &Value) -> Option<SearchHit> {
    let id = video["videoId"].as_str()?;
    Some(SearchHit {
        url: format!("https://www.youtube.com/watch?v={id}"),
        preview: LinkPreview {
            title: text(&video["title"])?,
            channel: text(&video["ownerText"]).or_else(|| text(&video["longBylineText"])),
            duration_ms: text(&video["lengthText"]).and_then(|t| clock_ms(&t)),
            thumbnail: video["thumbnail"]["thumbnails"][0]["url"].as_str().map(Into::into),
        },
    })
}

/// YouTube's autocomplete phrases for what's been typed so far.
pub fn suggestions(query: &str) -> Result<Vec<String>> {
    let body = client()
        .get("https://suggestqueries-clients6.youtube.com/complete/search")
        .timeout(Duration::from_secs(2))
        .query(&[("client", "youtube"), ("ds", "yt"), ("oe", "utf-8"), ("q", query)])
        .send()?
        .error_for_status()?
        .text()?;
    let mut phrases = parse_suggestions(&body)?;
    phrases.truncate(SUGGESTIONS);
    Ok(phrases)
}

/// The phrases in a YouTube autocomplete response (`window.google.ac.h([query, [[phrase, …], …], …])`).
pub fn parse_suggestions(body: &str) -> Result<Vec<String>> {
    let json = body.trim().trim_start_matches(|c| c != '(').trim_start_matches('(').trim_end_matches(')');
    let response: Value = serde_json::from_str(json)?;
    Ok(response[1].as_array().into_iter().flatten().filter_map(|s| s[0].as_str().map(Into::into)).collect())
}

/// "1:02:03" or "3:25" to milliseconds.
pub(super) fn clock_ms(clock: &str) -> Option<i64> {
    clock.split(':').try_fold(0i64, |total, part| Some(total * 60 + part.trim().parse::<i64>().ok()?)).map(|s| s * 1000)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_lists_videos_once_each_skipping_live_and_upcoming_ones() {
        let hits = parse_search(include_str!("youtube-search.sample.json")).unwrap();
        let hit = |id: &str, title: &str, channel: Option<&str>, duration_ms: Option<i64>, thumbnail: Option<&str>| SearchHit {
            url: format!("https://www.youtube.com/watch?v={id}"),
            preview: LinkPreview { title: title.into(), channel: channel.map(Into::into), duration_ms, thumbnail: thumbnail.map(Into::into) },
        };
        assert_eq!(
            hits,
            vec![
                hit("aaaaaaaaaaa", "Paper Boats (Made Up Video)", Some("Juniper Row"), Some(205_000), Some("https://i.ytimg.com/vi/aaaaaaaaaaa/small.jpg")),
                hit("bbbbbbbbbbb", "Overpass Karaoke Mix", Some("Dani Sato - Topic"), Some(3_723_000), Some("https://i.ytimg.com/vi/bbbbbbbbbbb/small.jpg")),
                hit("eeeeeeeeeee", "Old Made Up Stream", Some("Someone"), None, None),
            ]
        );
    }

    #[test]
    fn suggestions_read_youtubes_autocomplete_phrases() {
        let body = r#"window.google.ac.h(["paper b",[["paper boats",0,[512,433]],["paper boats karaoke",0,[512]],["\u7D19\u306E\u821F",0,[512]]],{"k":1}])"#;
        assert_eq!(parse_suggestions(body).unwrap(), ["paper boats", "paper boats karaoke", "紙の舟"]);
    }

    #[test]
    #[ignore = "network"]
    fn live_search_and_suggestions_answer_fast() {
        let start = std::time::Instant::now();
        let hits = search("lofi piano instrumental").unwrap();
        println!("search: {} hits in {:?}", hits.len(), start.elapsed());
        assert!(!hits.is_empty() && hits.iter().all(|h| h.url.starts_with("https://www.youtube.com/watch?v=")));
        let start = std::time::Instant::now();
        let phrases = suggestions("lofi pia").unwrap();
        println!("suggestions: {phrases:?} in {:?}", start.elapsed());
        assert!(!phrases.is_empty());
    }
}
