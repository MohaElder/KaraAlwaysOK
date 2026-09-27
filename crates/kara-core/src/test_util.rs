use crate::separate::mdx::{MdxParams, VocalModel};
use ndarray::Array4;
use std::path::Path;

/// Writes a 16-bit PCM WAV sine at half amplitude, the same sample on every channel.
pub(crate) fn write_sine_wav(path: &Path, rate: u32, channels: u16, secs: f32, freq: f32) {
    let spec = hound::WavSpec {
        channels,
        sample_rate: rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };
    let mut w = hound::WavWriter::create(path, spec).unwrap();
    let n = (rate as f32 * secs) as usize;
    for i in 0..n {
        let x = (i as f32 * freq * 2.0 * std::f32::consts::PI / rate as f32).sin() * 0.5;
        let s = (x * 32767.0) as i16;
        for _ in 0..channels {
            w.write_sample(s).unwrap();
        }
    }
    w.finalize().unwrap();
}

/// Tiny MDX shape so tests run in milliseconds: chunk 240, trim 32, gen 176.
pub(crate) fn test_params() -> MdxParams {
    MdxParams { n_fft: 64, hop: 16, dim_f: 33, dim_t: 16, compensate: 1.0 }
}

/// Says there are no vocals.
pub(crate) struct Silence;
impl VocalModel for Silence {
    fn infer(&mut self, input: Array4<f32>) -> anyhow::Result<Array4<f32>> {
        Ok(Array4::zeros(input.dim()))
    }
}

/// Says everything is vocals.
pub(crate) struct Passthrough;
impl VocalModel for Passthrough {
    fn infer(&mut self, input: Array4<f32>) -> anyhow::Result<Array4<f32>> {
        Ok(input)
    }
}

/// Sets BSD file flags, e.g. "uchg" (Finder's Locked) or "nouchg".
#[cfg(target_os = "macos")]
pub(crate) fn chflags(flag: &str, path: &Path) {
    assert!(std::process::Command::new("chflags").arg(flag).arg(path).status().unwrap().success());
}
