//! Bilibili's own web search and search suggestions, called directly.

use super::preview::{LinkPreview, SearchHit, SEARCH_RESULTS};
use super::youtube::{clock_ms, SUGGESTIONS};
use anyhow::{bail, Result};
use reqwest::header::REFERER;
use serde_json::Value;
use std::sync::OnceLock;
use std::time::Duration;

/// The desktop browser Bilibili's web API expects to be talking to.
const BROWSER: &str = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36";

/// One client for every call, keeping the cookies bilibili.com hands out for the app's life.
fn client() -> &'static reqwest::blocking::Client {
    static CLIENT: OnceLock<reqwest::blocking::Client> = OnceLock::new();
    CLIENT.get_or_init(|| reqwest::blocking::Client::builder().cookie_store(true).user_agent(BROWSER).build().expect("an HTTP client"))
}

/// The top Bilibili videos for `query`, getting fresh cookies from bilibili.com and asking again if refused.
pub fn search(query: &str) -> Result<Vec<SearchHit>> {
    let ask = || -> Result<Vec<SearchHit>> {
        let text = client()
            .get("https://api.bilibili.com/x/web-interface/search/type")
            .timeout(Duration::from_secs(5))
            .query(&[("search_type", "video"), ("keyword", query)])
            .header(REFERER, "https://search.bilibili.com/")
            .send()?
            .error_for_status()?
            .text()?;
        parse_search(&text)
    };
    let mut hits = ask().or_else(|_| {
        client().get("https://www.bilibili.com").timeout(Duration::from_secs(5)).send()?.error_for_status()?;
        ask()
    })?;
    hits.truncate(SEARCH_RESULTS);
    Ok(hits)
}

/// The videos in a Bilibili web search response, without highlight tags or HTML entities in their titles.
pub fn parse_search(json: &str) -> Result<Vec<SearchHit>> {
    let response: Value = serde_json::from_str(json)?;
    if response["code"] != 0 {
        bail!("Bilibili refused the search ({})", response["code"]);
    }
    Ok(response["data"]["result"].as_array().into_iter().flatten().filter(|v| v["type"] == "video").filter_map(hit).collect())
}

fn hit(video: &Value) -> Option<SearchHit> {
    let pic = video["pic"].as_str();
    Some(SearchHit {
        url: format!("https://www.bilibili.com/video/{}", video["bvid"].as_str()?),
        preview: LinkPreview {
            title: plain(video["title"].as_str()?),
            channel: video["author"].as_str().map(Into::into),
            duration_ms: video["duration"].as_str().and_then(clock_ms),
            thumbnail: pic.map(|p| p.strip_prefix("//").map_or(p.into(), |rest| format!("https://{rest}"))),
        },
    })
}

/// Text without its HTML tags and entities.
fn plain(html: &str) -> String {
    let mut text = String::new();
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => text.push(c),
            _ => {}
        }
    }
    quick_xml::escape::unescape(&text).map(|t| t.into_owned()).unwrap_or(text)
}

/// Bilibili's suggested phrases for what's been typed so far.
pub fn suggestions(query: &str) -> Result<Vec<String>> {
    let text = client()
        .get("https://s.search.bilibili.com/main/suggest")
        .timeout(Duration::from_secs(2))
        .query(&[("term", query)])
        .send()?
        .error_for_status()?
        .text()?;
    let mut phrases = parse_suggestions(&text)?;
    phrases.truncate(SUGGESTIONS);
    Ok(phrases)
}

/// The phrases in a Bilibili suggestion response.
pub fn parse_suggestions(json: &str) -> Result<Vec<String>> {
    let response: Value = serde_json::from_str(json)?;
    Ok(response["result"]["tag"].as_array().into_iter().flatten().filter_map(|t| t["value"].as_str().map(Into::into)).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn search_lists_videos_with_plain_titles_and_full_links() {
        let json = r#"{"code":0,"data":{"result":[
            {"type":"video","bvid":"BV1aaaaaaaaa","title":"<em class=\"keyword\">Paper</em> Boats &amp; Kites &#x27;Made Up&#x27;","author":"Juniper Row","duration":"4:30","pic":"//i0.hdslb.com/bfs/archive/a.jpg"},
            {"type":"ketang","bvid":"","title":"A Made Up Course","author":"Someone","duration":"","pic":"https://archive.biliimg.com/b.jpg"},
            {"type":"video","bvid":"BV1bbbbbbbbb","title":"Overpass Mix","author":"Dani Sato","duration":"62:03","pic":"https://i1.hdslb.com/bfs/archive/b.jpg"}
        ]}}"#;
        let hit = |id: &str, title: &str, channel: &str, duration_ms: i64, thumbnail: &str| SearchHit {
            url: format!("https://www.bilibili.com/video/{id}"),
            preview: LinkPreview { title: title.into(), channel: Some(channel.into()), duration_ms: Some(duration_ms), thumbnail: Some(thumbnail.into()) },
        };
        assert_eq!(
            parse_search(json).unwrap(),
            vec![
                hit("BV1aaaaaaaaa", "Paper Boats & Kites 'Made Up'", "Juniper Row", 270_000, "https://i0.hdslb.com/bfs/archive/a.jpg"),
                hit("BV1bbbbbbbbb", "Overpass Mix", "Dani Sato", 3_723_000, "https://i1.hdslb.com/bfs/archive/b.jpg"),
            ]
        );
        assert!(parse_search(r#"{"code":-412,"message":"request was banned"}"#).is_err());
    }

    #[test]
    fn suggestions_read_bilibilis_phrases() {
        let json = r#"{"code":0,"result":{"tag":[{"value":"paper boats","name":"<em>paper</em> boats"},{"value":"紙の舟 karaoke"}]}}"#;
        assert_eq!(parse_suggestions(json).unwrap(), ["paper boats", "紙の舟 karaoke"]);
    }

    #[test]
    #[ignore = "network"]
    fn live_search_and_suggestions_answer_fast() {
        let start = std::time::Instant::now();
        let hits = search("lofi piano instrumental").unwrap();
        println!("search: {} hits in {:?}", hits.len(), start.elapsed());
        assert!(!hits.is_empty() && hits.iter().all(|h| h.url.starts_with("https://www.bilibili.com/video/BV")));
        let start = std::time::Instant::now();
        let phrases = suggestions("lofi pia").unwrap();
        println!("suggestions: {phrases:?} in {:?}", start.elapsed());
        assert!(!phrases.is_empty());
    }
}
