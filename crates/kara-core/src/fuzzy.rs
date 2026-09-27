//! Fuzzy text matching in every language that ignores case, accents, full/half width and hiragana/katakana, and forgives small typos.

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use std::cmp::Reverse;
use unicode_normalization::UnicodeNormalization;

/// Lowercases, folds full/half width to one form, strips accents from letters and turns hiragana into katakana.
fn fold(text: &str) -> String {
    let hiragana_to_katakana = |c| if ('\u{3041}'..='\u{3096}').contains(&c) { char::from_u32(c as u32 + 0x60).unwrap_or(c) } else { c };
    text.to_lowercase().nfkd().filter(|c| !('\u{300}'..='\u{36f}').contains(c)).nfc().map(hiragana_to_katakana).collect()
}

/// One search query, ready to score texts against.
pub struct Fuzzy {
    words: Vec<String>,
    pattern: Pattern,
    matcher: Matcher,
    buf: Vec<char>,
}

impl Fuzzy {
    /// None when the query has no words; a query under 3 characters must appear in one piece.
    pub fn new(query: &str) -> Option<Self> {
        let query = fold(query);
        let kind = if query.trim().chars().count() < 3 { AtomKind::Substring } else { AtomKind::Fuzzy };
        let pattern = Pattern::new(&query, CaseMatching::Respect, Normalization::Never, kind);
        let words = query.split_whitespace().map(String::from).collect();
        (!pattern.atoms.is_empty()).then(|| Self { words, pattern, matcher: Matcher::new(Config::DEFAULT), buf: Vec::new() })
    }

    /// How well `text` holds every word of the query, in any order; None when it doesn't.
    /// A text that holds them only with one letter swapped or wrong in each long word scores lowest.
    pub fn score(&mut self, text: &str) -> Option<u32> {
        let text = fold(text);
        self.pattern.score(Utf32Str::new(&text, &mut self.buf), &mut self.matcher).or_else(|| self.near(&text).then_some(1))
    }

    /// Whether every word of 5+ characters is one edit from a word of `text` and every shorter word is in it.
    fn near(&self, text: &str) -> bool {
        self.words.iter().all(|w| {
            if w.chars().count() < 5 {
                text.contains(w.as_str())
            } else {
                text.split_whitespace().any(|t| strsim::osa_distance(w, t) <= 1)
            }
        })
    }
}

/// The items `score` matches, best first.
pub fn best_first<T>(items: impl IntoIterator<Item = T>, mut score: impl FnMut(&T) -> Option<u32>) -> Vec<T> {
    let mut hits: Vec<_> = items.into_iter().filter_map(|item| Some((Reverse(score(&item)?), item))).collect();
    hits.sort_by_key(|(score, _)| *score);
    hits.into_iter().map(|(_, item)| item).collect()
}
