//! Automatic lyric timing: slides the lyric timeline against when the separated
//! vocals are singing and picks the offset that lines them up, if one clearly does.

use crate::audio::SAMPLE_RATE;
use crate::cache;
use crate::lyrics::Line;
use crate::store::Store;
use anyhow::Result;

/// Length of one step of the singing and lyric timelines.
const HOP_MS: i64 = 50;
const HOPS_PER_S: i64 = 1_000 / HOP_MS;
/// Offsets tried: lyrics up to a minute earlier or two minutes later.
const EARLIEST: i64 = -60 * HOPS_PER_S;
const LATEST: i64 = 120 * HOPS_PER_S;
/// Lyrics and singing must overlap this long for an offset to count.
const MIN_OVERLAP: usize = 60 * HOPS_PER_S as usize;
/// The best offset's correlation must reach this…
const MIN_SCORE: f64 = 0.3;
/// …and every other peak more than a second away must stay below this share of it.
const MAX_RIVAL: f64 = 0.8;
/// Vocals quieter than this are silence, however quiet the song.
const SILENCE_DB: f32 = -50.0;
/// Singing is within this many dB of the song's loud singing.
const SINGING_RANGE_DB: f32 = 20.0;
/// Pauses up to this long between sung words still count as singing.
const BREATH_HOPS: usize = 600 / HOP_MS as usize;

/// Loudness of each hop of the first `chunks` stored vocal chunks, in dB.
pub fn vocal_loudness(store: &Store, hash: &str, model_id: &str, chunks: u32) -> Result<Vec<f32>> {
    const HOP: usize = (SAMPLE_RATE as i64 * HOP_MS / 1000) as usize;
    let (mut db, mut power, mut n) = (Vec::new(), 0.0f32, 0);
    for i in 0..chunks {
        let v = cache::read_vocals(store, hash, model_id, i)?;
        for (l, r) in v.left.iter().zip(&v.right) {
            power += ((l + r) / 2.0).powi(2);
            n += 1;
            if n == HOP {
                db.push(10.0 * (power / HOP as f32 + 1e-10).log10());
                (power, n) = (0.0, 0);
            }
        }
    }
    Ok(db)
}

/// Which hops are singing: within `SINGING_RANGE_DB` of the song's loud singing
/// (its 95th percentile) and louder than silence, with breaths filled in.
pub fn singing(db: &[f32]) -> Vec<bool> {
    let mut sorted = db.to_vec();
    sorted.sort_by(f32::total_cmp);
    let loud = sorted.get(((sorted.len() as f32 - 1.0) * 0.95).round() as usize).copied().unwrap_or(SILENCE_DB);
    let floor = (loud - SINGING_RANGE_DB).max(SILENCE_DB);
    let mut on: Vec<bool> = db.iter().map(|&d| d > floor).collect();
    let mut last_on = None;
    for i in 0..on.len() {
        if on[i] {
            if let Some(j) = last_on.filter(|&j| i - j <= BREATH_HOPS + 1) {
                on[j..i].fill(true);
            }
            last_on = Some(i);
        }
    }
    on
}

/// Which hops the lyrics say are sung: from each line's first word to its last,
/// or the whole line when it has no words.
pub fn lyric_activity(lines: &[Line]) -> Vec<bool> {
    let spans: Vec<(i64, i64)> = lines
        .iter()
        .map(|l| match (l.words.first(), l.words.last()) {
            (Some(first), Some(last)) => (first.start_ms, last.end_ms),
            _ => (l.start_ms, l.end_ms),
        })
        .collect();
    let hop = |ms: i64| (ms.max(0) / HOP_MS) as usize;
    let mut on = vec![false; spans.iter().map(|&(_, end)| hop(end)).max().unwrap_or(0)];
    for (start, end) in spans {
        on[hop(start).min(hop(end))..hop(end)].fill(true);
    }
    on
}

/// The lyric offset in ms (positive = lyrics later) that best lines the lyrics up
/// with the singing, when it is clearly better than any other offset.
pub fn best_offset(sung: &[bool], lyric: &[bool]) -> Option<i64> {
    let scores: Vec<f64> = (EARLIEST..=LATEST).map(|d| correlation(sung, lyric, d)).collect();
    let (best_i, &best) = scores.iter().enumerate().max_by(|a, b| a.1.total_cmp(b.1))?;
    let second = HOPS_PER_S as usize;
    let is_peak = |i: usize| scores[i.saturating_sub(second)..(i + second + 1).min(scores.len())].iter().all(|&s| s <= scores[i]);
    let rival = (0..scores.len()).filter(|&i| i.abs_diff(best_i) > second && is_peak(i)).map(|i| scores[i]).fold(0.0, f64::max);
    (best >= MIN_SCORE && rival < best * MAX_RIVAL).then(|| (EARLIEST + best_i as i64) * HOP_MS)
}

/// Pearson correlation of `lyric[i]` with `sung[i + shift]` where both exist; 0 when they overlap too little or either is constant.
fn correlation(sung: &[bool], lyric: &[bool], shift: i64) -> f64 {
    let start = (-shift).max(0) as usize;
    let end = (sung.len() as i64 - shift).clamp(0, lyric.len() as i64) as usize;
    if end < start + MIN_OVERLAP {
        return 0.0;
    }
    let (mut l, mut s, mut both) = (0u32, 0u32, 0u32);
    for i in start..end {
        let (x, y) = (lyric[i], sung[(i as i64 + shift) as usize]);
        l += x as u32;
        s += y as u32;
        both += (x && y) as u32;
    }
    let n = (end - start) as f64;
    let (l, s, both) = (l as f64 / n, s as f64 / n, both as f64 / n);
    let spread = (l * (1.0 - l) * s * (1.0 - s)).sqrt();
    if spread == 0.0 { 0.0 } else { (both - l * s) / spread }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::Word;

    /// xorshift64: repeatable made-up timings.
    struct Rng(u64);
    impl Rng {
        fn below(&mut self, n: u64) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0 % n
        }
    }

    fn line(start_ms: i64, sung_ms: i64, end_ms: i64) -> Line {
        let words = vec![Word { start_ms, end_ms: start_ms + sung_ms, text: "la".into() }];
        Line { start_ms, end_ms, text: "la".into(), words, voice: None }
    }

    /// Three minutes of lines with irregular lengths and gaps.
    fn made_up_lines(seed: u64) -> Vec<Line> {
        let mut rng = Rng(seed);
        let mut t = 8_000;
        let mut lines: Vec<Line> = Vec::new();
        while t < 170_000 {
            let sung = 1_000 + rng.below(60) as i64 * HOP_MS;
            let next = t + sung + 300 + rng.below(54) as i64 * HOP_MS;
            lines.push(line(t, sung, next));
            t = next;
        }
        lines
    }

    /// `lyric` moved later by `ms`, with one hop in ten flipped.
    fn singing_like(lyric: &[bool], ms: i64, seed: u64) -> Vec<bool> {
        let shift = ms / HOP_MS;
        let mut rng = Rng(seed);
        (0..lyric.len() as i64 + 200)
            .map(|i| {
                let on = usize::try_from(i - shift).ok().and_then(|j| lyric.get(j).copied()).unwrap_or(false);
                on ^ (rng.below(10) == 0)
            })
            .collect()
    }

    #[test]
    fn singing_is_near_the_songs_loud_parts_bridges_breaths_and_is_never_silence() {
        let db: Vec<f32> = [(-10.0, 10), (-40.0, 12), (-10.0, 10), (-40.0, 13), (-25.0, 10)].iter().flat_map(|&(d, n)| std::iter::repeat_n(d, n)).collect();
        let s = singing(&db);
        assert!(s[..32].iter().all(|&x| x) && s[32..45].iter().all(|&x| !x) && s[45..].iter().all(|&x| x));
        assert!(singing(&[-70.0; 20]).iter().all(|s| !s));
    }

    #[test]
    fn lyric_activity_runs_from_the_first_to_the_last_sung_word() {
        let a = lyric_activity(&[line(100, 100, 1_000), Line { words: vec![], ..line(300, 0, 400) }]);
        assert_eq!(a, [false, false, true, true, false, false, true, true]);
    }

    #[test]
    fn a_known_shift_is_found_both_ways() {
        let lyric = lyric_activity(&made_up_lines(7));
        for ms in [2_350, -1_800, 45_000, -30_000] {
            assert_eq!(best_offset(&singing_like(&lyric, ms, 3), &lyric), Some(ms));
        }
    }

    #[test]
    fn a_partly_prepared_song_is_enough() {
        let lyric = lyric_activity(&made_up_lines(11));
        let sung = singing_like(&lyric, 3_200, 5);
        assert_eq!(best_offset(&sung[..90_000 / HOP_MS as usize], &lyric), Some(3_200));
    }

    #[test]
    fn unrelated_singing_gives_no_offset() {
        let lyric = lyric_activity(&made_up_lines(7));
        let other = lyric_activity(&made_up_lines(99));
        assert_eq!(best_offset(&singing_like(&other, 0, 3), &lyric), None);
    }

    #[test]
    fn a_repeating_pattern_gives_no_offset() {
        let lines: Vec<Line> = (0..45).map(|i| line(i * 4_000, 2_000, i * 4_000 + 4_000)).collect();
        let lyric = lyric_activity(&lines);
        assert_eq!(best_offset(&singing_like(&lyric, 1_300, 3), &lyric), None);
    }

    #[test]
    fn nothing_to_line_up_gives_no_offset() {
        let lyric = lyric_activity(&made_up_lines(7));
        assert_eq!(best_offset(&vec![false; 4_000], &lyric), None);
        assert_eq!(best_offset(&singing_like(&lyric, 0, 3), &[]), None);
    }
}
