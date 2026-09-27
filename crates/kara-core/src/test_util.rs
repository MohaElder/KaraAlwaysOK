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
