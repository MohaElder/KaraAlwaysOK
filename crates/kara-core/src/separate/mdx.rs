//! Runs an MDX-Net vocal model over a whole song in segments and regroups the
//! result into fixed-length chunks (vocals + instrumental = mix).

use super::stft::{Complex32, Stft};
use crate::audio::Stereo;
use anyhow::{bail, Result};
use ndarray::Array4;
use std::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct MdxParams {
    pub n_fft: usize,
    pub hop: usize,
    pub dim_f: usize,
    pub dim_t: usize,
    pub compensate: f32,
}

impl MdxParams {
    /// Samples the model sees per segment.
    pub fn chunk_size(&self) -> usize {
        self.hop * (self.dim_t - 1)
    }
    /// Edge samples discarded on each side of a segment.
    pub fn trim(&self) -> usize {
        self.n_fft / 2
    }
    /// Useful output samples per segment.
    pub fn gen_size(&self) -> usize {
        self.chunk_size() - 2 * self.trim()
    }
}

pub trait VocalModel: Send {
    /// Input and output shape: [1, 4, dim_f, dim_t] (L.re, L.im, R.re, R.im).
    fn infer(&mut self, input: Array4<f32>) -> Result<Array4<f32>>;
}

pub struct ChunkOut {
    pub index: usize,
    pub vocals: Stereo,
    pub inst: Stereo,
}

#[derive(Debug, PartialEq, Eq)]
pub enum Outcome {
    Done,
    Cancelled,
}

pub fn chunk_count(total: usize, chunk_len: usize) -> usize {
    total.div_ceil(chunk_len)
}

/// Packs two channels' spectra into the model input, dropping bins above
/// `dim_f`. Bins 0..3 are left zero, matching UVR's own inference (it zeroes
/// them right before running the model).
fn pack(l: &[Vec<Complex32>], r: &[Vec<Complex32>], dim_f: usize) -> Array4<f32> {
    let mut a = Array4::<f32>::zeros((1, 4, dim_f, l.len()));
    for (c, spec) in [l, r].into_iter().enumerate() {
        for (t, frame) in spec.iter().enumerate() {
            for f in 3..dim_f {
                a[[0, c * 2, f, t]] = frame[f].re;
                a[[0, c * 2 + 1, f, t]] = frame[f].im;
            }
        }
    }
    a
}

/// Model output → two channels' spectra, zero-filled up to `n_bins`.
fn unpack(a: &Array4<f32>, n_bins: usize) -> (Vec<Vec<Complex32>>, Vec<Vec<Complex32>>) {
    let (_, _, dim_f, frames) = a.dim();
    let channel = |c: usize| -> Vec<Vec<Complex32>> {
        (0..frames)
            .map(|t| {
                let mut v = vec![Complex32::new(0.0, 0.0); n_bins];
                for f in 0..dim_f.min(n_bins) {
                    v[f] = Complex32::new(a[[0, c * 2, f, t]], a[[0, c * 2 + 1, f, t]]);
                }
                v
            })
            .collect()
    };
    (channel(0), channel(1))
}

fn padded_slice(x: &[f32], base: usize, size: usize, trim: usize) -> Vec<f32> {
    (0..size)
        .map(|j| (base + j).checked_sub(trim).and_then(|i| x.get(i)).copied().unwrap_or(0.0))
        .collect()
}

/// Separates `mix`, calling `on_chunk` for chunks `start_chunk..` in order.
/// Checks `cancel` before every model segment.
pub fn separate(
    model: &mut dyn VocalModel,
    p: &MdxParams,
    mix: &Stereo,
    chunk_len: usize,
    start_chunk: usize,
    cancel: &AtomicBool,
    mut on_chunk: impl FnMut(ChunkOut) -> Result<()>,
) -> Result<Outcome> {
    let total = mix.len();
    let out_start = start_chunk * chunk_len;
    if out_start >= total {
        return Ok(Outcome::Done);
    }
    let (gen, trim, size) = (p.gen_size(), p.trim(), p.chunk_size());
    let n_bins = p.n_fft / 2 + 1;
    let stft = Stft::new(p.n_fft, p.hop);
    let mut seg = out_start / gen;
    let mut skip = out_start - seg * gen;
    let mut index = start_chunk;
    let (mut pv, mut pi) = (Stereo::default(), Stereo::default());

    while seg * gen < total {
        if cancel.load(Ordering::Relaxed) {
            return Ok(Outcome::Cancelled);
        }
        let base = seg * gen;
        let l = padded_slice(&mix.left, base, size, trim);
        let r = padded_slice(&mix.right, base, size, trim);
        let output = model.infer(pack(&stft.forward(&l), &stft.forward(&r), p.dim_f))?;
        let want = (1, 4, p.dim_f, p.dim_t);
        if output.dim() != want {
            bail!("model returned shape {:?}, expected {:?}", output.dim(), want);
        }
        let (sl, sr) = unpack(&output, n_bins);
        let (vl, vr) = (stft.inverse(&sl, size), stft.inverse(&sr, size));
        let n = (base + gen).min(total) - base;
        for k in skip..n {
            let (a, b) = (vl[trim + k] * p.compensate, vr[trim + k] * p.compensate);
            pv.left.push(a);
            pv.right.push(b);
            pi.left.push(mix.left[base + k] - a);
            pi.right.push(mix.right[base + k] - b);
        }
        skip = 0;
        seg += 1;
        let last = seg * gen >= total;
        while pv.len() >= chunk_len || (last && !pv.is_empty()) {
            let take = chunk_len.min(pv.len());
            let vocals = Stereo { left: pv.left.drain(..take).collect(), right: pv.right.drain(..take).collect() };
            let inst = Stereo { left: pi.left.drain(..take).collect(), right: pi.right.drain(..take).collect() };
            on_chunk(ChunkOut { index, vocals, inst })?;
            index += 1;
        }
    }
    Ok(Outcome::Done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::{test_params, Passthrough, Silence};

    fn mix(len: usize) -> Stereo {
        // Period 8 = exactly FFT bin 8 of 64, so a passthrough model keeps it intact.
        let left: Vec<f32> = (0..len).map(|i| (i as f32 * std::f32::consts::PI / 4.0).sin() * 0.5).collect();
        let right = left.iter().map(|x| x * 0.8).collect();
        Stereo { left, right }
    }

    fn run(model: &mut dyn VocalModel, p: &MdxParams, m: &Stereo, chunk_len: usize, start: usize) -> Vec<ChunkOut> {
        let mut out = Vec::new();
        let r = separate(model, p, m, chunk_len, start, &AtomicBool::new(false), |c| {
            out.push(c);
            Ok(())
        })
        .unwrap();
        assert_eq!(r, Outcome::Done);
        out
    }

    fn joined(chunks: &[ChunkOut], vocals: bool) -> Stereo {
        let mut s = Stereo::default();
        for c in chunks {
            s.append(if vocals { &c.vocals } else { &c.inst });
        }
        s
    }

    #[test]
    fn params_derive_segment_sizes() {
        let p = test_params();
        assert_eq!((p.chunk_size(), p.trim(), p.gen_size()), (240, 32, 176));
        assert_eq!(chunk_count(1000, 300), 4);
        assert_eq!(chunk_count(0, 300), 0);
    }

    #[test]
    fn silence_model_gives_instrumental_equal_to_mix() {
        let m = mix(1000);
        let chunks = run(&mut Silence, &test_params(), &m, 300, 0);
        assert_eq!(chunks.iter().map(|c| c.vocals.len()).collect::<Vec<_>>(), vec![300, 300, 300, 100]);
        assert_eq!(chunks.iter().map(|c| c.index).collect::<Vec<_>>(), vec![0, 1, 2, 3]);
        assert_eq!(joined(&chunks, false), m);
        assert!(joined(&chunks, true).left.iter().all(|x| *x == 0.0));
    }

    #[test]
    fn vocals_plus_instrumental_is_the_mix() {
        let m = mix(1000);
        let p = MdxParams { compensate: 1.035, ..test_params() };
        let chunks = run(&mut Passthrough, &p, &m, 300, 0);
        let (v, i) = (joined(&chunks, true), joined(&chunks, false));
        for k in 0..m.len() {
            assert!((v.left[k] + i.left[k] - m.left[k]).abs() < 1e-6);
            assert!((v.right[k] + i.right[k] - m.right[k]).abs() < 1e-6);
        }
    }

    /// What a whole-signal STFT round trip gives after zeroing bins 0..3, the
    /// same zeroing `pack` applies. A passthrough model should reproduce this,
    /// not the raw mix, since the lowest bins never reach the model.
    fn low_bins_zeroed(x: &[f32], p: &MdxParams) -> Vec<f32> {
        let stft = Stft::new(p.n_fft, p.hop);
        let mut spec = stft.forward(x);
        for frame in &mut spec {
            frame[..3].fill(Complex32::new(0.0, 0.0));
        }
        stft.inverse(&spec, x.len())
    }

    #[test]
    fn passthrough_output_lines_up_with_the_input() {
        let m = mix(2000);
        let p = test_params();
        let chunks = run(&mut Passthrough, &p, &m, 300, 0);
        let v = joined(&chunks, true);
        let expected = low_bins_zeroed(&m.left, &p);
        assert_eq!(v.len(), m.len());
        // Segment seams leave a small residual (< 4e-3 measured); a real
        // alignment bug (e.g. an off-by-one trim offset) misses by ~0.35.
        for (k, (got, want)) in v.left.iter().zip(&expected).enumerate().take(1900).skip(100) {
            assert!((got - want).abs() < 4e-3, "sample {k}: {got} vs {want}");
        }
    }

    #[test]
    fn resume_from_a_later_chunk_matches_a_full_run() {
        let m = mix(1500);
        let full = run(&mut Passthrough, &test_params(), &m, 300, 0);
        let resumed = run(&mut Passthrough, &test_params(), &m, 300, 2);
        assert_eq!(resumed[0].index, 2);
        assert_eq!(resumed.len(), full.len() - 2);
        for (a, b) in full[2..].iter().zip(&resumed) {
            assert_eq!(a.vocals, b.vocals);
            assert_eq!(a.inst, b.inst);
        }
    }

    #[test]
    fn short_input_makes_one_chunk() {
        let m = mix(50);
        let chunks = run(&mut Passthrough, &test_params(), &m, 300, 0);
        assert_eq!(chunks.len(), 1);
        assert_eq!(chunks[0].vocals.len(), 50);
    }

    #[test]
    fn cancel_stops_before_any_chunk() {
        let m = mix(1000);
        let mut n = 0;
        let r = separate(&mut Silence, &test_params(), &m, 300, 0, &AtomicBool::new(true), |_| {
            n += 1;
            Ok(())
        })
        .unwrap();
        assert_eq!((r, n), (Outcome::Cancelled, 0));
    }

    #[test]
    fn wrong_model_output_shape_is_an_error() {
        struct Bad;
        impl VocalModel for Bad {
            fn infer(&mut self, _: Array4<f32>) -> anyhow::Result<Array4<f32>> {
                Ok(Array4::zeros((1, 4, 2, 2)))
            }
        }
        let r = separate(&mut Bad, &test_params(), &mix(500), 300, 0, &AtomicBool::new(false), |_| Ok(()));
        assert!(r.is_err());
    }
}
