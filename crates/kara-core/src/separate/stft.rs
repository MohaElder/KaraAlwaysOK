//! STFT / inverse STFT matching torch.stft(center=True, reflect padding,
//! periodic Hann window), as used to train UVR's MDX-Net models.

use realfft::{ComplexToReal, RealFftPlanner, RealToComplex};
use std::sync::Arc;

pub use realfft::num_complex::Complex32;

pub struct Stft {
    n_fft: usize,
    hop: usize,
    window: Vec<f32>,
    fwd: Arc<dyn RealToComplex<f32>>,
    inv: Arc<dyn ComplexToReal<f32>>,
}

impl Stft {
    pub fn new(n_fft: usize, hop: usize) -> Self {
        let mut planner = RealFftPlanner::<f32>::new();
        let window = (0..n_fft)
            .map(|i| 0.5 - 0.5 * (2.0 * std::f32::consts::PI * i as f32 / n_fft as f32).cos())
            .collect();
        Self { n_fft, hop, window, fwd: planner.plan_fft_forward(n_fft), inv: planner.plan_fft_inverse(n_fft) }
    }

    /// Frames produced for a signal of `len` samples (center=True).
    pub fn frames(&self, len: usize) -> usize {
        1 + len / self.hop
    }

    /// One channel → frames × (n_fft/2 + 1) complex bins.
    pub fn forward(&self, x: &[f32]) -> Vec<Vec<Complex32>> {
        let pad = self.n_fft / 2;
        assert!(x.len() > pad, "signal shorter than half an FFT window");
        let padded = reflect_pad(x, pad);
        let mut buf = self.fwd.make_input_vec();
        (0..self.frames(x.len()))
            .map(|f| {
                let start = f * self.hop;
                for i in 0..self.n_fft {
                    buf[i] = padded[start + i] * self.window[i];
                }
                let mut spec = self.fwd.make_output_vec();
                self.fwd.process(&mut buf, &mut spec).expect("fft sizes match");
                spec
            })
            .collect()
    }

    /// Weighted overlap-add inverse, trimmed back to `len` samples.
    pub fn inverse(&self, frames: &[Vec<Complex32>], len: usize) -> Vec<f32> {
        let n = self.n_fft;
        let pad = n / 2;
        let total = n + self.hop * (frames.len() - 1);
        let mut out = vec![0f32; total];
        let mut wsum = vec![0f32; total];
        let mut spec = self.inv.make_input_vec();
        let mut buf = self.inv.make_output_vec();
        let scale = 1.0 / n as f32;
        for (f, frame) in frames.iter().enumerate() {
            spec.copy_from_slice(frame);
            let last = spec.len() - 1;
            spec[0].im = 0.0; // DC and Nyquist must be real for a real inverse
            spec[last].im = 0.0;
            self.inv.process(&mut spec, &mut buf).expect("fft sizes match");
            let start = f * self.hop;
            for i in 0..n {
                let w = self.window[i];
                out[start + i] += buf[i] * scale * w;
                wsum[start + i] += w * w;
            }
        }
        for (o, w) in out.iter_mut().zip(&wsum) {
            if *w > 1e-8 {
                *o /= *w;
            }
        }
        out[pad..pad + len].to_vec()
    }
}

fn reflect_pad(x: &[f32], pad: usize) -> Vec<f32> {
    let n = x.len();
    let mut v = Vec::with_capacity(n + 2 * pad);
    v.extend((1..=pad).rev().map(|i| x[i]));
    v.extend_from_slice(x);
    v.extend((1..=pad).map(|i| x[n - 1 - i]));
    v
}

#[cfg(test)]
mod tests {
    use super::*;

    fn signal(len: usize) -> Vec<f32> {
        (0..len).map(|i| (i as f32 * 0.3).sin() * 0.4 + (i as f32 * 0.071).cos() * 0.3).collect()
    }

    #[test]
    fn roundtrip_is_exact() {
        let s = Stft::new(64, 16);
        let x = signal(1000);
        let y = s.inverse(&s.forward(&x), x.len());
        assert_eq!(y.len(), x.len());
        let max = x.iter().zip(&y).fold(0f32, |m, (a, b)| m.max((a - b).abs()));
        assert!(max < 1e-4, "max err {max}");
    }

    #[test]
    fn frame_count_matches_mdx_segment() {
        let s = Stft::new(64, 16);
        // MDX segment length is hop * (dim_t - 1); it must give exactly dim_t frames.
        assert_eq!(s.frames(16 * 15), 16);
        let spec = s.forward(&signal(16 * 15));
        assert_eq!(spec.len(), 16);
        assert_eq!(spec[0].len(), 33);
    }

    // Reference bins for n_fft=16, hop=4, signal(40), generated with
    // torch.stft(x, n_fft=16, hop_length=4, win_length=16,
    // window=torch.hann_window(16, periodic=True), center=True,
    // pad_mode="reflect", onesided=True, return_complex=True).
    // Generator script pasted in the task-2 fix report.
    #[test]
    fn matches_torch_reference() {
        let s = Stft::new(16, 4);
        let x = signal(40);
        assert_eq!(s.frames(x.len()), 11);
        let spec = s.forward(&x);
        assert_eq!(spec.len(), 11);
        assert_eq!(spec[0].len(), 9);

        // Frame 0: boundary frame, content depends on reflect padding.
        let frame0 = [
            Complex32::new(4.155554, 0.0),
            Complex32::new(-1.653265, 0.0),
            Complex32::new(-0.650103, 0.0),
            Complex32::new(0.291102, 0.0),
            Complex32::new(-0.125849, 0.0),
            Complex32::new(0.096629, 0.0),
            Complex32::new(-0.071493, 0.0),
            Complex32::new(0.065534, 0.0),
            Complex32::new(-0.060662, 0.0),
        ];
        // Frame 5: interior frame, unaffected by padding.
        let frame5 = [
            Complex32::new(-0.251363, 0.0),
            Complex32::new(0.276157, 1.075579),
            Complex32::new(-0.138527, -0.451060),
            Complex32::new(-0.008315, -0.026128),
            Complex32::new(-0.002083, -0.006975),
            Complex32::new(-0.000796, -0.002777),
            Complex32::new(-0.000396, -0.001265),
            Complex32::new(-0.000252, -0.000514),
            Complex32::new(-0.000214, 0.0),
        ];

        for (got, want) in spec[0].iter().zip(&frame0) {
            assert!((got.re - want.re).abs() < 1e-4, "re: got {got:?} want {want:?}");
            assert!((got.im - want.im).abs() < 1e-4, "im: got {got:?} want {want:?}");
        }
        for (got, want) in spec[5].iter().zip(&frame5) {
            assert!((got.re - want.re).abs() < 1e-4, "re: got {got:?} want {want:?}");
            assert!((got.im - want.im).abs() < 1e-4, "im: got {got:?} want {want:?}");
        }
    }
}
