//! Phone mics: each phone's jitter buffer, gain and feedback control, summed through a shared reverb and a limiter.

mod buffer;
mod howl;

pub use buffer::JitterBuffer;
pub use howl::Howl;
use std::sync::Arc;

pub const MAX_GAIN: f32 = 2.0;
const WET: f32 = 0.2;
const CEILING: f32 = 0.89;

/// A phone's gain from the Mac's volume slider and the phone's Voice slider (0–100 each), never above MAX_GAIN.
pub fn voice_gain(volume: u8, voice: u8) -> f32 {
    MAX_GAIN * f32::from(volume.min(100)) / 100.0 * f32::from(voice.min(100)) / 100.0
}

/// Samples one output callback may ask for; each phone's scratch buffer is this big from the start.
const MOST_FRAMES: usize = 4096;
const MOST_PHONES: usize = 8;

#[derive(Clone)]
pub struct Level {
    pub id: Arc<str>,
    pub peak: f32,
    pub down: bool,
}

/// A phone's id, buffer and feedback control, made before taking the mixer lock.
pub struct NewVoice {
    id: Arc<str>,
    buffer: JitterBuffer,
    howl: Howl,
}

impl NewVoice {
    pub fn new(id: &str, in_rate: u32, out_rate: u32, floor_ms: f64) -> Self {
        Self { id: id.into(), buffer: JitterBuffer::new(in_rate, out_rate, floor_ms), howl: Howl::new(out_rate) }
    }
}

struct Voice {
    id: Arc<str>,
    buffer: JitterBuffer,
    howl: Howl,
    gain: f32,
    peak: f32,
    scratch: Vec<f32>,
}

/// A removed phone's voice, freed wherever the caller drops it.
pub struct Gone {
    _voice: Voice,
}

pub struct Mixer {
    rate: u32,
    floor_ms: f64,
    voices: Vec<Voice>,
    reverb: Reverb,
    limiter: Limiter,
}

impl Mixer {
    pub fn new(rate: u32, floor_ms: f64) -> Self {
        Self { rate, floor_ms, voices: Vec::with_capacity(MOST_PHONES), reverb: Reverb::new(rate), limiter: Limiter::new(rate) }
    }

    pub fn rate(&self) -> u32 {
        self.rate
    }

    pub fn floor_ms(&self) -> f64 {
        self.floor_ms
    }

    /// Starts taking a phone's sound. A phone already here swaps in only the fresh buffer and keeps its gain and feedback state;
    /// what it doesn't use comes back, to be dropped after unlocking.
    pub fn add(&mut self, mut new: NewVoice) -> Option<NewVoice> {
        let id = new.id.clone();
        if let Some(v) = self.voice(&id) {
            std::mem::swap(&mut v.buffer, &mut new.buffer);
            return Some(new);
        }
        let NewVoice { id, buffer, howl } = new;
        self.voices.push(Voice { id, buffer, howl, gain: 1.0, peak: 0.0, scratch: Vec::with_capacity(MOST_FRAMES) });
        None
    }

    /// Takes a phone out of the mix; drop what comes back after unlocking.
    pub fn remove(&mut self, id: &str) -> Option<Gone> {
        let i = self.voices.iter().position(|v| &*v.id == id)?;
        Some(Gone { _voice: self.voices.swap_remove(i) })
    }

    pub fn push(&mut self, id: &str, samples: &[f32]) {
        if let Some(v) = self.voice(id) {
            v.buffer.push(samples);
        }
    }

    pub fn reset(&mut self, id: &str) {
        if let Some(v) = self.voice(id) {
            v.buffer.reset();
        }
    }

    pub fn set_gain(&mut self, id: &str, gain: f32) {
        if let Some(v) = self.voice(id) {
            v.gain = gain.min(MAX_GAIN);
        }
    }

    /// Mixes the next `out.len()` samples of every phone, with reverb, never above the ceiling.
    pub fn render(&mut self, out: &mut [f32]) {
        out.fill(0.0);
        for v in &mut self.voices {
            v.scratch.resize(out.len(), 0.0);
            v.buffer.pull(&mut v.scratch);
            v.howl.feed(&v.scratch);
            let gain = v.gain * v.howl.gain();
            for (o, s) in out.iter_mut().zip(&v.scratch) {
                let x = s * gain;
                v.peak = v.peak.max(x.abs());
                *o += x;
            }
        }
        for o in out.iter_mut() {
            *o = self.limiter.process(*o + WET * self.reverb.process(*o));
        }
    }

    /// Each phone's loudest moment since the last call, into `out` (cleared first; its room is reused).
    pub fn levels(&mut self, out: &mut Vec<Level>) {
        out.clear();
        out.extend(self.voices.iter_mut().map(|v| Level { id: v.id.clone(), peak: std::mem::take(&mut v.peak), down: v.howl.down() }));
    }

    fn voice(&mut self, id: &str) -> Option<&mut Voice> {
        self.voices.iter_mut().find(|v| &*v.id == id)
    }
}

const COMB_INPUT: f32 = 0.25;
const COMB_FEEDBACK: f32 = 0.78;
const COMB_DAMP: f32 = 0.25;
const ALLPASS_FEEDBACK: f32 = 0.5;

struct Comb {
    buf: Vec<f32>,
    i: usize,
    low: f32,
}

struct Allpass {
    buf: Vec<f32>,
    i: usize,
}

/// A small room: four damped combs into two allpasses, the Freeverb layout.
struct Reverb {
    combs: Vec<Comb>,
    allpasses: Vec<Allpass>,
}

impl Reverb {
    fn new(rate: u32) -> Self {
        let len = |n: usize| n * rate as usize / 44_100;
        Self {
            combs: [1116, 1188, 1277, 1356].map(|n| Comb { buf: vec![0.0; len(n)], i: 0, low: 0.0 }).into(),
            allpasses: [556, 441].map(|n| Allpass { buf: vec![0.0; len(n)], i: 0 }).into(),
        }
    }

    fn process(&mut self, x: f32) -> f32 {
        let mut out = 0.0;
        for c in &mut self.combs {
            let y = c.buf[c.i];
            c.low = y * (1.0 - COMB_DAMP) + c.low * COMB_DAMP;
            c.buf[c.i] = x * COMB_INPUT + c.low * COMB_FEEDBACK;
            c.i = (c.i + 1) % c.buf.len();
            out += y;
        }
        for a in &mut self.allpasses {
            let y = a.buf[a.i];
            a.buf[a.i] = out + y * ALLPASS_FEEDBACK;
            a.i = (a.i + 1) % a.buf.len();
            out = y - out;
        }
        out
    }
}

/// Keeps the mix under the ceiling: turns down at once, comes back over about 80 ms.
struct Limiter {
    gain: f32,
    release: f32,
}

impl Limiter {
    fn new(rate: u32) -> Self {
        Self { gain: 1.0, release: 1.0 - (-1.0 / (0.08 * rate as f32)).exp() }
    }

    fn process(&mut self, x: f32) -> f32 {
        let room = if x.abs() > CEILING { CEILING / x.abs() } else { 1.0 };
        self.gain = room.min(self.gain + (1.0 - self.gain) * self.release);
        x * self.gain
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn four_phones_at_full_volume_never_pass_the_ceiling_and_the_reverb_dies_away() {
        assert_eq!(voice_gain(255, 255), MAX_GAIN);
        let ids = ["a", "b", "c", "d"];
        let mut m = Mixer::new(48_000, 20.0);
        for id in ids {
            m.add(NewVoice::new(id, 48_000, 48_000, 20.0));
            m.set_gain(id, voice_gain(100, 100));
        }
        let loud: Vec<f32> = (0..48_000).map(|i| if (i / 80) % 2 == 0 { 0.99 } else { -0.99 }).collect();
        let mut out = vec![0.0; 4 * 48_000];
        for (i, block) in out.chunks_mut(256).enumerate() {
            if let Some(part) = loud.get(i * 256..(i + 1) * 256) {
                for id in ids {
                    m.push(id, part);
                }
            }
            m.render(block);
        }
        let peak = out.iter().fold(0f32, |p, x| p.max(x.abs()));
        assert!(peak > 0.5 && peak <= CEILING + 1e-6, "peak {peak}");
        assert!(out[out.len() - 4_800..].iter().all(|x| x.abs() < 1e-3), "the reverb tail dies away");
    }
}
