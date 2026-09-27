//! A phone's voice effect before the mix: none, a karaoke mix (short echo and warm reverb), or auto-tune (pulled to the nearest semitone).

use super::{flush, Reverb};
use serde::Deserialize;
use std::collections::VecDeque;

const ECHO_MS: f32 = 120.0;
const ECHO_FEEDBACK: f32 = 0.35;
const ECHO_WET: f32 = 0.35;
const ROOM_WET: f32 = 0.5;
const WARM_HZ: f32 = 4_000.0;
/// How long switching effects or intensities takes, so a change never clicks.
const RAMP_MS: f32 = 20.0;
const LOW_HZ: f32 = 80.0;
const HIGH_HZ: f32 = 1_000.0;
const DECIMATE: usize = 4;
/// Decimated samples compared per pitch estimate (about 21 ms).
const WINDOW: usize = 256;
const LOOK_EVERY: usize = 256;
const YIN_THRESHOLD: f32 = 0.15;
const QUIET: f32 = 1e-5;
/// The auto-tune's taps restart in turn every whole number of periods, at least this long apart.
const MIN_SPACING_MS: f32 = 3.5;
const REST_SPACING_MS: f32 = 5.0;
const LINE_MS: f32 = 40.0;

#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Effect {
    #[default]
    None,
    KaraokeMix,
    AutoTune,
}

pub struct Effects {
    /// The effect playing now, and the one asked for (with its intensity 0 to 1).
    active: Effect,
    wanted: (Effect, f32),
    amount: f32,
    wet: f32,
    ramp: f32,
    echo: Vec<f32>,
    echo_i: usize,
    room: Reverb,
    warm: f32,
    warm_k: f32,
    tune: Tune,
}

impl Effects {
    pub fn new(rate: u32) -> Self {
        Self {
            active: Effect::None,
            wanted: (Effect::None, 0.0),
            amount: 0.0,
            wet: 0.0,
            ramp: 1.0 / (RAMP_MS / 1000.0 * rate as f32),
            echo: vec![0.0; (ECHO_MS / 1000.0 * rate as f32) as usize],
            echo_i: 0,
            room: Reverb::new(rate),
            warm: 0.0,
            warm_k: 1.0 - (-std::f32::consts::TAU * WARM_HZ / rate as f32).exp(),
            tune: Tune::new(rate),
        }
    }

    /// Asks for an effect and its intensity, 0–100; intensity 0 means no effect. The change fades in over a few milliseconds.
    pub fn set(&mut self, effect: Effect, amount: u8) {
        let amount = f32::from(amount.min(100)) / 100.0;
        self.wanted = (if amount == 0.0 { Effect::None } else { effect }, amount);
    }

    /// Applies the effect to `samples` in place, fading between the effect playing and the one asked for.
    pub fn process(&mut self, samples: &mut [f32]) {
        for x in samples {
            let (want, amount) = self.wanted;
            if want != self.active && self.wet == 0.0 {
                self.active = want;
                self.tune.clear();
            }
            let target = if want == self.active { 1.0 } else { 0.0 };
            self.wet = if self.wet < target { (self.wet + self.ramp).min(target) } else { (self.wet - self.ramp).max(target) };
            if want == self.active {
                self.amount = if self.amount < amount { (self.amount + self.ramp).min(amount) } else { (self.amount - self.ramp).max(amount) };
                self.tune.set(self.amount);
            }
            let dry = *x;
            let room = self.karaoke(dry);
            let wet = match self.active {
                Effect::AutoTune => self.tune.next(dry),
                other => {
                    self.tune.hear(dry);
                    if other == Effect::KaraokeMix { room } else { dry }
                }
            };
            *x = dry + self.wet * (wet - dry);
        }
    }

    /// The voice with its echo and warm room; runs whatever the effect, so turning karaoke on replays nothing old.
    fn karaoke(&mut self, x: f32) -> f32 {
        let echoed = self.echo[self.echo_i];
        self.echo[self.echo_i] = flush(x + echoed * ECHO_FEEDBACK);
        self.echo_i = (self.echo_i + 1) % self.echo.len();
        self.warm = flush(self.warm + (self.room.process(x) - self.warm) * self.warm_k);
        x + self.amount * (ECHO_WET * echoed + ROOM_WET * self.warm)
    }
}

/// Chromatic pitch correction: finds the pitch of the recent input and replays it through two taps whose delays slide at the
/// corrected speed; each tap restarts a whole number of periods from the other, so their crossfades stay in phase.
struct Tune {
    rate: f32,
    line: Vec<f32>,
    write: usize,
    phase: f32,
    life: f32,
    delay: [f32; 2],
    period: f32,
    spacing: f32,
    ratio: f32,
    target: f32,
    strength: f32,
    glide: f32,
    history: VecDeque<f32>,
    sum: f32,
    count: usize,
    until_look: usize,
    cmnd: Vec<f32>,
    low_tau: usize,
    high_tau: usize,
}

impl Tune {
    fn new(rate: u32) -> Self {
        let rate = rate as f32;
        let slow = rate / DECIMATE as f32;
        let high_tau = (slow / LOW_HZ) as usize;
        let spacing = REST_SPACING_MS / 1000.0 * rate;
        Self {
            rate,
            line: vec![0.0; (LINE_MS / 1000.0 * rate) as usize],
            write: 0,
            phase: 0.0,
            life: 2.0 * spacing,
            delay: [spacing; 2],
            period: spacing,
            spacing,
            ratio: 1.0,
            target: 1.0,
            strength: 0.0,
            glide: 0.0,
            history: VecDeque::with_capacity(WINDOW + high_tau),
            sum: 0.0,
            count: 0,
            until_look: LOOK_EVERY,
            cmnd: vec![1.0; high_tau + 2],
            low_tau: (slow / HIGH_HZ) as usize,
            high_tau,
        }
    }

    /// Keeps the recent input while auto-tune is off, so turning it on replays nothing old.
    fn hear(&mut self, x: f32) {
        self.line[self.write] = x;
        self.write = (self.write + 1) % self.line.len();
    }

    /// Starts listening afresh with the taps at rest.
    fn clear(&mut self) {
        self.history.clear();
        self.spacing = REST_SPACING_MS / 1000.0 * self.rate;
        self.period = self.spacing;
        self.delay = [self.spacing; 2];
        self.life = 2.0 * self.spacing;
        self.phase = 0.0;
        self.ratio = 1.0;
        self.target = 1.0;
    }

    /// How far (0 to 1) and how fast the pitch is pulled.
    fn set(&mut self, amount: f32) {
        self.strength = amount;
        self.glide = 1.0 / ((0.05 - 0.045 * amount) * self.rate);
    }

    fn next(&mut self, x: f32) -> f32 {
        self.listen(x);
        self.shift(x)
    }

    /// Keeps a short, decimated history; every few milliseconds aims the ratio at the nearest semitone and spaces the taps by whole periods.
    fn listen(&mut self, x: f32) {
        self.sum += x;
        self.count += 1;
        if self.count == DECIMATE {
            if self.history.len() == WINDOW + self.high_tau {
                self.history.pop_front();
            }
            self.history.push_back(self.sum / DECIMATE as f32);
            self.sum = 0.0;
            self.count = 0;
        }
        self.until_look -= 1;
        if self.until_look == 0 {
            self.until_look = LOOK_EVERY;
            self.target = match self.pitch() {
                Some(hz) => {
                    self.period = self.rate / hz;
                    self.spacing = self.period * (MIN_SPACING_MS / 1000.0 * self.rate / self.period).ceil();
                    let semis = 12.0 * (hz / 440.0).log2();
                    2f32.powf((semis.round() - semis) * self.strength / 12.0)
                }
                None => 1.0,
            };
        }
        self.ratio += (self.target - self.ratio) * self.glide;
    }

    /// The recent input's pitch in Hz (YIN), or None when it is quiet or not a clear note.
    fn pitch(&mut self) -> Option<f32> {
        if self.history.len() < WINDOW + self.high_tau {
            return None;
        }
        let h = self.history.make_contiguous();
        if h.iter().map(|x| x * x).sum::<f32>() / (h.len() as f32) < QUIET {
            return None;
        }
        let mut total = 0.0;
        for tau in 1..=self.high_tau {
            let d: f32 = (0..WINDOW).map(|j| (h[j] - h[j + tau]).powi(2)).sum();
            total += d;
            self.cmnd[tau] = if total > 0.0 { d * tau as f32 / total } else { 1.0 };
        }
        let mut tau = (self.low_tau.max(2)..self.high_tau).find(|&t| self.cmnd[t] < YIN_THRESHOLD)?;
        while tau + 1 < self.high_tau && self.cmnd[tau + 1] < self.cmnd[tau] {
            tau += 1;
        }
        if tau + 1 >= self.high_tau {
            return None;
        }
        let (a, b, c) = (self.cmnd[tau - 1], self.cmnd[tau], self.cmnd[tau + 1]);
        let bend = a - 2.0 * b + c;
        let shift = if bend.abs() > 1e-9 { (0.5 * (a - c) / bend).clamp(-0.5, 0.5) } else { 0.0 };
        Some(self.rate / DECIMATE as f32 / (tau as f32 + shift))
    }

    /// Plays the two taps crossfaded over their overlapping lives; their delays slide by 1 - ratio a sample, which moves the pitch by `ratio`.
    fn shift(&mut self, x: f32) -> f32 {
        let len = self.line.len();
        self.line[self.write] = x;
        let most = (len - 2) as f32;
        for d in &mut self.delay {
            *d = (*d + 1.0 - self.ratio).clamp(0.0, most);
        }
        let before = self.phase;
        self.phase += 1.0 / self.life;
        if before < 0.5 && self.phase >= 0.5 {
            self.restart(1);
        }
        if self.phase >= 1.0 {
            self.phase -= 1.0;
            self.restart(0);
        }
        let w = 1.0 - (2.0 * self.phase - 1.0).abs();
        let y = w * self.read(self.delay[0]) + (1.0 - w) * self.read(self.delay[1]);
        self.write = (self.write + 1) % len;
        y
    }

    /// Starts tap `i` again at the shortest delay, a whole number of periods from the other, that stays clear of zero for its life.
    fn restart(&mut self, i: usize) {
        let (other, s) = (self.delay[1 - i], self.spacing);
        let least = 0.1 * s;
        self.delay[i] = other - ((other - least) / self.period).floor() * self.period;
        self.life = 2.0 * s;
    }

    fn read(&self, delay: f32) -> f32 {
        let len = self.line.len();
        let at = (self.write as f32 - delay).rem_euclid(len as f32);
        let i = (at as usize) % len;
        let f = at - at.floor();
        self.line[i] * (1.0 - f) + self.line[(i + 1) % len] * f
    }

    /// How far behind the voice the taps are now, weighted by how loud each is, in samples.
    #[cfg(test)]
    fn delay_now(&self) -> f32 {
        let w = 1.0 - (2.0 * self.phase - 1.0).abs();
        w * self.delay[0] + (1.0 - w) * self.delay[1]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::f32::consts::TAU;

    const RATE: f32 = 48_000.0;
    const PITCHES: [f32; 5] = [98.0, 110.0, 131.0, 262.0, 440.0];

    fn sine(hz: f32, secs: f32) -> Vec<f32> {
        (0..(secs * RATE) as usize).map(|i| 0.5 * (TAU * hz * i as f32 / RATE).sin()).collect()
    }

    /// A made-up sung note: ten harmonics with a slight vibrato.
    fn voice(hz: f32, secs: f32) -> Vec<f32> {
        let mut phase = 0.0f32;
        (0..(secs * RATE) as usize)
            .map(|i| {
                phase += TAU * hz * (1.0 + 0.004 * (TAU * 5.0 * i as f32 / RATE).sin()) / RATE;
                0.3 * (1..=10).map(|h| (phase * h as f32).sin() / h as f32).sum::<f32>()
            })
            .collect()
    }

    /// The frequency of `x`, from its rising zero crossings.
    fn hz(x: &[f32]) -> f32 {
        let ups: Vec<f32> = x.windows(2).enumerate().filter(|(_, w)| w[0] < 0.0 && w[1] >= 0.0).map(|(i, w)| i as f32 + w[0] / (w[0] - w[1])).collect();
        (ups.len() - 1) as f32 * RATE / (ups[ups.len() - 1] - ups[0])
    }

    fn cents(a: f32, b: f32) -> f32 {
        1200.0 * (a / b).log2()
    }

    /// The semitone nearest `hz`.
    fn note(hz: f32) -> f32 {
        440.0 * 2f32.powf((12.0 * (hz / 440.0).log2()).round() / 12.0)
    }

    fn run(fx: &mut Effects, mut x: Vec<f32>) -> Vec<f32> {
        for block in x.chunks_mut(256) {
            fx.process(block);
        }
        x
    }

    fn with(effect: Effect, amount: u8, x: Vec<f32>) -> Vec<f32> {
        let mut fx = Effects::new(48_000);
        fx.set(effect, amount);
        run(&mut fx, x)
    }

    /// No step between neighboring samples bigger than a quiet 440 Hz tone with its echoes makes; a click is several times that.
    fn smooth(out: &[f32]) -> bool {
        out.windows(2).all(|w| (w[1] - w[0]).abs() < 0.06)
    }

    #[test]
    fn auto_tune_pulls_low_and_high_notes_to_the_nearest_semitone_as_hard_as_asked() {
        for sung in PITCHES {
            let target = note(sung);
            for amount in [100u8, 50] {
                for off in [-30.0, 30.0] {
                    let out = hz(&with(Effect::AutoTune, amount, sine(target * 2f32.powf(off / 1200.0), 2.5))[(0.4 * RATE) as usize..]);
                    let (now, want) = (cents(out, target), off * (1.0 - f32::from(amount) / 100.0));
                    assert!((now - want).abs() < 3.0, "{sung} Hz sung {off} cents off at intensity {amount}: {now:.1} cents off, want {want}");
                }
            }
        }
    }

    #[test]
    fn auto_tune_delays_most_voices_about_12_ms_and_low_voices_at_most_25() {
        for sung in PITCHES {
            let mut t = Tune::new(48_000);
            t.set(1.0);
            let mut most = 0.0f32;
            for x in sine(note(sung) * 2f32.powf(-30.0 / 1200.0), 2.0) {
                t.next(x);
                most = most.max(t.delay_now());
            }
            let bound = if sung >= 200.0 { 12.0 } else { 25.0 };
            assert!(most / RATE * 1000.0 <= bound, "{sung} Hz: delayed {:.1} ms while correcting", most / RATE * 1000.0);
        }
    }

    #[test]
    fn auto_tune_keeps_going_through_low_sung_notes_and_reads_right_at_the_edge_of_its_line() {
        let t = Tune::new(48_000);
        assert!(t.read(1e-6).is_finite(), "a delay a hair above zero at the start of the line");
        let mut fx = Effects::new(48_000);
        fx.set(Effect::AutoTune, 100);
        for sung in [110.0, 147.0, 196.0, 220.0] {
            for bend in [0.985, 1.0, 1.012] {
                assert!(run(&mut fx, voice(sung * bend, 2.0)).iter().all(|x| x.is_finite()));
            }
        }
    }

    #[test]
    fn bass_notes_at_the_floor_stay_finite_and_notes_below_it_are_left_alone() {
        for tenth in 760..=800 {
            let out = with(Effect::AutoTune, 100, voice(tenth as f32 / 10.0, 2.0));
            assert!(out.iter().all(|x| x.is_finite()), "{} Hz", tenth as f32 / 10.0);
        }
        let sung = 76.0 * 2f32.powf(-40.0 / 1200.0);
        let out = hz(&with(Effect::AutoTune, 100, sine(sung, 2.5))[(0.4 * RATE) as usize..]);
        assert!(cents(out, sung).abs() < 1.0, "76 Hz sung 40 cents flat moved {:.1} cents", cents(out, sung));
    }

    #[test]
    fn karaoke_mix_is_dry_at_zero_and_adds_a_room_and_an_echo_when_up() {
        let mut click = vec![0.0; 48_000];
        click[0] = 1.0;
        assert_eq!(with(Effect::KaraokeMix, 0, click.clone()), click, "dry at intensity 0");
        assert_eq!(with(Effect::AutoTune, 0, click.clone()), click, "auto-tune is dry at intensity 0 too");
        assert_eq!(with(Effect::None, 100, click.clone()), click, "no effect is dry");
        let mut fx = Effects::new(48_000);
        fx.set(Effect::KaraokeMix, 100);
        run(&mut fx, vec![0.0; 2_400]);
        let wet = run(&mut fx, click);
        let room = wet[(0.03 * RATE) as usize..(0.1 * RATE) as usize].iter().map(|x| x * x).sum::<f32>();
        assert!(room > 1e-4, "the room answers before the first echo");
        assert!(wet[(ECHO_MS / 1000.0 * RATE) as usize].abs() > 0.2, "an echo after {ECHO_MS} ms");
        run(&mut fx, voice(220.0, 1.0));
        fx.set(Effect::None, 100);
        run(&mut fx, vec![0.0; 480_000]);
        fx.set(Effect::KaraokeMix, 100);
        assert!(run(&mut fx, vec![0.0; 48_000]).iter().all(|x| x.abs() < 1e-3), "turning it back on replays nothing old");
    }

    #[test]
    fn switching_effects_or_their_strength_never_clicks() {
        let mut fx = Effects::new(48_000);
        let mut tone: Vec<f32> = sine(440.0 * 2f32.powf(-20.0 / 1200.0), 2.0).iter().map(|x| 0.4 * x).collect();
        let changes = [(Effect::AutoTune, 100), (Effect::KaraokeMix, 100), (Effect::KaraokeMix, 0), (Effect::AutoTune, 60), (Effect::None, 100)];
        for (part, (effect, amount)) in tone.chunks_mut((0.4 * RATE) as usize).zip(changes) {
            fx.set(effect, amount);
            for block in part.chunks_mut(256) {
                fx.process(block);
            }
        }
        assert!(smooth(&tone));
    }
}
