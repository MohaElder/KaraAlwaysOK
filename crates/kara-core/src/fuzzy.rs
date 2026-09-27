//! Fuzzy text matching in every language, ignoring case, accents and full/half width.

use nucleo_matcher::pattern::{AtomKind, CaseMatching, Normalization, Pattern};
use nucleo_matcher::{Config, Matcher, Utf32Str};
use std::cmp::Reverse;
use unicode_normalization::UnicodeNormalization;

/// Folds full/half width to one form and strips accents from letters.
fn fold(text: &str) -> String {
    text.nfkd().filter(|c| !('\u{300}'..='\u{36f}').contains(c)).nfc().collect()
}

/// One search query, ready to score texts against.
pub struct Fuzzy {
    pattern: Pattern,
    matcher: Matcher,
    buf: Vec<char>,
}

impl Fuzzy {
    /// None when the query has no words.
    pub fn new(query: &str) -> Option<Self> {
        let pattern = Pattern::new(&fold(query), CaseMatching::Ignore, Normalization::Never, AtomKind::Fuzzy);
        (!pattern.atoms.is_empty()).then(|| Self { pattern, matcher: Matcher::new(Config::DEFAULT), buf: Vec::new() })
    }

    /// How well `text` holds every word of the query, in any order; None when it doesn't.
    pub fn score(&mut self, text: &str) -> Option<u32> {
        self.pattern.score(Utf32Str::new(&fold(text), &mut self.buf), &mut self.matcher)
    }
}

/// The items `score` matches, best first.
pub fn best_first<T>(items: impl IntoIterator<Item = T>, mut score: impl FnMut(&T) -> Option<u32>) -> Vec<T> {
    let mut hits: Vec<_> = items.into_iter().filter_map(|item| Some((Reverse(score(&item)?), item))).collect();
    hits.sort_by_key(|(score, _)| *score);
    hits.into_iter().map(|(_, item)| item).collect()
}
