//! Feedback control for one mic: spots a single pure tone that lasts and keeps growing (or already screams) and turns the mic down until it settles.

use realfft::num_complex::Complex32;
use realfft::{RealFftPlanner, RealToComplex};
use std::ops::Range;
use std::sync::Arc;

const N: usize = 1024;
const LOW_HZ: f32 = 150.0;
const HIGH_HZ: f32 = 8_000.0;
/// Frames (about a quarter second at 48 kHz) a tone must last before the mic is turned down.
const RUN: u32 = 12;
const DOMINANCE: f32 = 100.0;
const LOUD: f32 = 0.01;
const SCREAMING: f32 = 0.5;
const GROWN: f32 = 4.0;
const CUT: f32 = 0.5;
const MIN_GAIN: f32 = 1.0 / 16.0;
/// Frames (about two seconds) without a tone before the gain starts coming back.
const SETTLE: u32 = 96;
const RECOVER: f32 = 1.012;

pub struct Howl {
    fft: Arc<dyn RealToComplex<f32>>,
    window: Vec<f32>,
    window_sum: f32,
    frame: Vec<f32>,
    input: Vec<f32>,
    spectrum: Vec<Complex32>,
    scratch: Vec<Complex32>,
    bins: Range<usize>,
    bin: usize,
    run: u32,
    first_power: f32,
    quiet: u32,
    gain: f32,
}

impl Howl {
    pub fn new(rate: u32) -> Self {
        let fft = RealFftPlanner::<f32>::new().plan_fft_forward(N);
        let window: Vec<f32> = (0..N).map(|i| 0.5 - 0.5 * (std::f32::consts::TAU * i as f32 / N as f32).cos()).collect();
        let hz = rate as f32 / N as f32;
        Self {
            window_sum: window.iter().sum(),
            input: fft.make_input_vec(),
            spectrum: fft.make_output_vec(),
            scratch: fft.make_scratch_vec(),
            bins: (LOW_HZ / hz) as usize..(HIGH_HZ / hz) as usize,
            fft,
            window,
            frame: Vec::with_capacity(N),
            bin: 0,
            run: 0,
            first_power: 0.0,
            quiet: 0,
            gain: 1.0,
        }
    }

    /// The mic's gain: 1, or less while it is turned down.
    pub fn gain(&self) -> f32 {
        self.gain
    }

    pub fn down(&self) -> bool {
        self.gain < 0.99
    }

    pub fn feed(&mut self, samples: &[f32]) {
        for &s in samples {
            self.frame.push(s);
            if self.frame.len() == N {
                self.analyze();
                self.frame.clear();
            }
        }
    }

    /// Looks at one frame: a lasting single tone that grew or screams cuts the gain; two quiet seconds let it come back.
    fn analyze(&mut self) {
        for ((x, s), w) in self.input.iter_mut().zip(&self.frame).zip(&self.window) {
            *x = s * w;
        }
        if self.fft.process_with_scratch(&mut self.input, &mut self.spectrum, &mut self.scratch).is_err() {
            return;
        }
        let power = |k: usize| self.spectrum[k].norm_sqr();
        let Some((bin, peak)) = self.bins.clone().map(|k| (k, power(k))).max_by(|a, b| a.1.total_cmp(&b.1)) else { return };
        let others = self.bins.clone().filter(|k| k.abs_diff(bin) > 2).map(power).fold(0.0, f32::max);
        let amplitude = 2.0 * peak.sqrt() / self.window_sum;
        if amplitude < LOUD || peak < DOMINANCE * others || peak < power(bin - 1).max(power(bin + 1)) {
            self.run = 0;
            self.quiet += 1;
            if self.quiet >= SETTLE {
                self.gain = (self.gain * RECOVER).min(1.0);
            }
            return;
        }
        self.quiet = 0;
        if self.run == 0 || bin.abs_diff(self.bin) > 1 {
            self.run = 0;
            self.first_power = peak;
        }
        self.run += 1;
        self.bin = bin;
        if self.run >= RUN && (peak >= GROWN * self.first_power || amplitude >= SCREAMING) {
            self.gain = (self.gain * CUT).max(MIN_GAIN);
            self.run = 0;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const RATE: f32 = 48_000.0;

    /// A repeatable hiss at about -50 dBFS.
    fn hiss(n: usize) -> Vec<f32> {
        let mut s = 12_345u32;
        (0..n)
            .map(|_| {
                s = s.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
                (s >> 8) as f32 / (1u32 << 24) as f32 * 0.006 - 0.003
            })
            .collect()
    }

    /// `secs` of a `hz` sine whose amplitude follows `amp(t)`, over the hiss.
    fn tone(hz: f32, secs: f32, amp: impl Fn(f32) -> f32) -> Vec<f32> {
        hiss((secs * RATE) as usize)
            .into_iter()
            .enumerate()
            .map(|(i, h)| {
                let t = i as f32 / RATE;
                h + amp(t) * (TAU * hz * t).sin()
            })
            .collect()
    }

    /// Seconds until `h` turned the mic down while hearing `signal`, if it did.
    fn turned_down_after(h: &mut Howl, signal: &[f32]) -> Option<f32> {
        signal
            .chunks(256)
            .position(|c| {
                h.feed(c);
                h.down()
            })
            .map(|i| (i * 256) as f32 / RATE)
    }

    #[test]
    fn a_growing_or_screaming_tone_is_turned_down_within_a_second_then_comes_back() {
        let growing = tone(2_500.0, 2.0, |t| (0.01 * 10f32.powf(1.5 * t)).min(0.9));
        let screaming = tone(2_500.0, 2.0, |_| 0.8);
        for (what, signal) in [("growing", growing), ("screaming", screaming)] {
            let mut h = Howl::new(48_000);
            let when = turned_down_after(&mut h, &signal).unwrap_or(f32::INFINITY);
            assert!(when <= 1.0, "{what}: turned down after {when} s");
            h.feed(&hiss(8 * 48_000));
            assert!(!h.down(), "{what}: back to full once it settled");
        }
    }

    /// Five seconds of a `hz` voice with vibrato and 12 harmonics falling as 1/h^`rolloff`, swelling for a second then held.
    fn sung(hz: f32, rolloff: f32) -> Vec<f32> {
        let mut voice = hiss((5.0 * RATE) as usize);
        let mut phase = 0.0f32;
        for (i, x) in voice.iter_mut().enumerate() {
            let t = i as f32 / RATE;
            phase += TAU * hz * (1.0 + 0.02 * (TAU * 5.5 * t).sin()) / RATE;
            let crescendo_then_held = 0.02 + 0.08 * t.min(1.0);
            *x += crescendo_then_held * (1..=12).map(|h| (phase * h as f32).sin() / (h as f32).powf(rolloff)).sum::<f32>();
        }
        voice
    }

    #[test]
    fn singing_and_a_steady_held_note_are_left_alone() {
        let mut h = Howl::new(48_000);
        assert_eq!(turned_down_after(&mut h, &sung(220.0, 1.0)), None, "singing");
        assert_eq!(turned_down_after(&mut h, &sung(100.0, 3.0)), None, "a soft low note");
        assert_eq!(turned_down_after(&mut h, &tone(1_000.0, 3.0, |_| 0.1)), None, "a held note");
    }
}
