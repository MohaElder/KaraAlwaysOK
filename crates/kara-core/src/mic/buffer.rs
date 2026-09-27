//! A phone's jitter buffer: holds its samples, resamples them to the output rate, and keeps the delay near a target that adapts to the Wi-Fi.

use std::collections::VecDeque;

const FADE_MS: f64 = 5.0;
const GROW_MS: f64 = 10.0;
const MAX_TARGET_MS: f64 = 60.0;
const SHRINK_MS: f64 = 2.0;
const STEADY_SECS: f64 = 10.0;
const TRIM_OVER_MS: f64 = 30.0;
const MAX_SKEW: f64 = 0.005;
/// The most sound the buffer ever holds; far above the 60 ms target plus a trimmed burst.
const HOLD_MS: f64 = 200.0;

pub struct JitterBuffer {
    queue: VecDeque<f32>,
    cap: usize,
    /// Read position in `queue`; at least 1 so the interpolation has a sample behind it.
    pos: f64,
    step: f64,
    in_rate: f64,
    out_rate: f64,
    floor_ms: f64,
    target_ms: f64,
    avg_fill_ms: f64,
    playing: bool,
    fade_in: f64,
    steady_secs: f64,
    /// A jump ahead after a burst: where to, and how far the crossfade has got (0 to 1).
    jump: Option<(f64, f64)>,
}

impl JitterBuffer {
    pub fn new(in_rate: u32, out_rate: u32, floor_ms: f64) -> Self {
        let cap = (HOLD_MS / 1000.0 * in_rate as f64) as usize;
        let mut queue = VecDeque::with_capacity(cap);
        queue.push_back(0.0);
        Self {
            queue,
            cap,
            pos: 1.0,
            step: in_rate as f64 / out_rate as f64,
            in_rate: in_rate as f64,
            out_rate: out_rate as f64,
            floor_ms,
            target_ms: floor_ms,
            avg_fill_ms: floor_ms,
            playing: false,
            fade_in: 0.0,
            steady_secs: 0.0,
            jump: None,
        }
    }

    /// Adds a phone's samples; when more than HOLD_MS would be waiting (nobody is reading), it starts over instead of growing.
    pub fn push(&mut self, samples: &[f32]) {
        let samples = &samples[samples.len().saturating_sub(self.cap - 1)..];
        if self.queue.len() + samples.len() > self.cap {
            self.reset();
        }
        self.queue.extend(samples);
    }

    /// Drops everything waiting; it fills up to the target again before playing.
    pub fn reset(&mut self) {
        self.queue.clear();
        self.queue.push_back(0.0);
        self.pos = 1.0;
        self.playing = false;
        self.jump = None;
    }

    /// Milliseconds of sound waiting to play.
    pub fn fill_ms(&self) -> f64 {
        (self.queue.len() as f64 - self.pos - 2.0).max(0.0) / self.in_rate * 1000.0
    }

    pub fn target_ms(&self) -> f64 {
        self.target_ms
    }

    /// Fills `out` at the output rate: silence while filling up, a short fade instead of a click when the sound runs out.
    pub fn pull(&mut self, out: &mut [f32]) {
        let fade = FADE_MS / 1000.0 * self.out_rate;
        for o in out {
            *o = 0.0;
            let fill = self.fill_ms();
            if !self.playing {
                if fill < self.target_ms {
                    continue;
                }
                self.playing = true;
                self.fade_in = 0.0;
                self.avg_fill_ms = fill;
            }
            let left = fill / 1000.0 * self.out_rate;
            if left < 1.0 {
                self.playing = false;
                self.target_ms = (self.target_ms + GROW_MS).min(MAX_TARGET_MS);
                self.steady_secs = 0.0;
                continue;
            }
            if self.jump.is_none() && fill > self.target_ms + TRIM_OVER_MS {
                self.jump = Some((self.pos + (fill - self.target_ms) / 1000.0 * self.in_rate, 0.0));
                self.avg_fill_ms = self.target_ms;
            }
            self.avg_fill_ms += (fill - self.avg_fill_ms) / self.out_rate;
            let skew = ((self.avg_fill_ms - self.target_ms) / self.target_ms * 0.01).clamp(-MAX_SKEW, MAX_SKEW);
            let mut x = self.at(self.pos);
            if let Some((to, k)) = self.jump {
                x = x * (1.0 - k) as f32 + self.at(to) * k as f32;
            }
            self.fade_in = (self.fade_in + 1.0 / fade).min(1.0);
            *o = x * self.fade_in.min(left / fade) as f32;
            self.advance(self.step * (1.0 + skew), fade);
            self.steady_secs += 1.0 / self.out_rate;
            if self.steady_secs >= STEADY_SECS {
                self.steady_secs = 0.0;
                self.target_ms = (self.target_ms - SHRINK_MS).max(self.floor_ms);
            }
        }
    }

    fn advance(&mut self, step: f64, fade: f64) {
        self.pos += step;
        if let Some((to, k)) = &mut self.jump {
            *to += step;
            *k += 1.0 / fade;
            if *k >= 1.0 {
                self.pos = *to;
                self.jump = None;
            }
        }
        let done = self.pos as usize - 1;
        if done > 0 {
            self.queue.drain(..done);
            self.pos -= done as f64;
            if let Some((to, _)) = &mut self.jump {
                *to -= done as f64;
            }
        }
    }

    /// The sound at fractional position `p`, by cubic interpolation.
    fn at(&self, p: f64) -> f32 {
        let i = p as usize;
        let t = (p - i as f64) as f32;
        let [a, b, c, d] = [i - 1, i, i + 1, i + 2].map(|k| self.queue.get(k).copied().unwrap_or(0.0));
        let c1 = 0.5 * (c - a);
        let c2 = a - 2.5 * b + 2.0 * c - 0.5 * d;
        let c3 = 0.5 * (d - a) + 1.5 * (b - c);
        ((c3 * t + c2) * t + c1) * t + b
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const OUT: u32 = 48_000;
    const FRAME: usize = 256;

    struct Run {
        out: Vec<f32>,
        /// (seconds, fill ms, target ms) after each output block.
        trace: Vec<(f64, f64, f64)>,
        buffer: JitterBuffer,
    }

    /// Plays `secs` of a 440 Hz tone from a phone that says it sends at `nominal` Hz while its clock makes `actual`;
    /// `arrive` says when a frame finished at a given time reaches the Mac.
    fn run(nominal: u32, actual: f64, secs: f64, arrive: impl Fn(f64) -> f64) -> Run {
        let mut frames = Vec::new();
        let mut n = 0;
        while n as f64 / actual < secs {
            let frame: Vec<f32> = (n..n + FRAME).map(|i| 0.5 * (i as f32 * 440.0 * std::f32::consts::TAU / nominal as f32).sin()).collect();
            n += FRAME;
            frames.push((arrive(n as f64 / actual), frame));
        }
        frames.sort_by(|a, b| a.0.total_cmp(&b.0));
        let mut buffer = JitterBuffer::new(nominal, OUT, 20.0);
        let (mut out, mut trace, mut next) = (Vec::new(), Vec::new(), 0);
        let mut block = [0.0; FRAME];
        for j in 0..(secs * OUT as f64 / FRAME as f64) as usize {
            let t = (j * FRAME) as f64 / OUT as f64;
            while next < frames.len() && frames[next].0 <= t {
                buffer.push(&frames[next].1);
                next += 1;
            }
            buffer.pull(&mut block);
            out.extend_from_slice(&block);
            trace.push((t, buffer.fill_ms(), buffer.target_ms()));
        }
        Run { out, trace, buffer }
    }

    /// No step between neighboring samples bigger than the tone itself makes; a click is about 0.5.
    fn smooth(out: &[f32]) -> bool {
        out.windows(2).all(|w| (w[1] - w[0]).abs() < 0.06)
    }

    #[test]
    fn keeps_the_delay_steady_when_the_phone_clock_drifts_or_runs_at_44_1_khz() {
        for (nominal, actual) in [(48_000, 48_048.0), (48_000, 47_952.0), (44_100, 44_100.0)] {
            let r = run(nominal, actual, 60.0, |t| t + 0.004);
            assert!(r.out[..(0.015 * OUT as f64) as usize].iter().all(|&x| x == 0.0), "{nominal}/{actual}: silent while filling up");
            assert!(smooth(&r.out), "{nominal}/{actual}: no clicks");
            assert_eq!(r.buffer.target_ms(), 20.0, "{nominal}/{actual}: never ran dry");
            let off = r.trace.iter().filter(|(t, ..)| *t > 30.0).map(|(_, fill, _)| (fill - 20.0).abs()).fold(0.0, f64::max);
            assert!(off < 8.0, "{nominal}/{actual}: the delay stays near 20 ms, it was off by {off:.1} ms");
        }
    }

    #[test]
    fn a_phone_nobody_hears_holds_little_memory() {
        let mut b = JitterBuffer::new(48_000, 48_000, 20.0);
        let room = b.queue.capacity();
        for _ in 0..10 * 48_000 / 256 {
            b.push(&[0.1; 256]);
        }
        assert!(b.fill_ms() <= HOLD_MS && b.queue.capacity() == room, "10 s unread stays within {HOLD_MS} ms without growing");
    }

    #[test]
    fn a_wifi_stall_fades_out_grows_the_target_and_a_burst_is_trimmed_back() {
        let r = run(48_000, 48_000.0, 70.0, |t| if (5.0..5.15).contains(&t) { 5.15 } else { t + 0.004 });
        assert!(smooth(&r.out), "fades instead of clicking");
        let (_, _, target) = *r.trace.iter().find(|(t, ..)| *t >= 6.0).unwrap();
        assert_eq!(target, 30.0, "the stall grew the target");
        let late = r.trace.iter().filter(|(t, ..)| (5.5..6.0).contains(t)).map(|(_, fill, _)| *fill).fold(0.0, f64::max);
        assert!(late < 40.0, "the burst was trimmed back near the target, the delay was {late:.1} ms");
        assert_eq!(r.buffer.target_ms(), 20.0, "a steady minute shrinks the target back to the floor");
    }
}
