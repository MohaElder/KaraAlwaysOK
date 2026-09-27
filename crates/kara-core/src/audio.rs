//! Everything becomes one standard format: 44.1 kHz stereo f32 in memory,
//! 16-bit FLAC on disk. Also tag reading and content hashing.

use anyhow::{anyhow, Context, Result};
use rubato::{FftFixedIn, Resampler};
use sha2::{Digest, Sha256};
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error as SymError;
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::{MetadataOptions, StandardTagKey, Tag};
use symphonia::core::probe::Hint;

pub const SAMPLE_RATE: u32 = 44_100;

/// Planar stereo audio at `SAMPLE_RATE`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Stereo {
    pub left: Vec<f32>,
    pub right: Vec<f32>,
}

impl Stereo {
    pub fn silence(len: usize) -> Self {
        Self { left: vec![0.0; len], right: vec![0.0; len] }
    }
    pub fn len(&self) -> usize {
        self.left.len()
    }
    pub fn is_empty(&self) -> bool {
        self.left.is_empty()
    }
    pub fn duration_ms(&self) -> i64 {
        self.len() as i64 * 1000 / SAMPLE_RATE as i64
    }
    pub fn slice(&self, start: usize, end: usize) -> Stereo {
        Stereo { left: self.left[start..end].to_vec(), right: self.right[start..end].to_vec() }
    }
    pub fn append(&mut self, other: &Stereo) {
        self.left.extend_from_slice(&other.left);
        self.right.extend_from_slice(&other.right);
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tags {
    pub title: Option<String>,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub lyrics: Option<String>,
}

pub struct Decoded {
    pub audio: Stereo,
    pub tags: Tags,
}

fn open(path: &Path) -> Result<(Box<dyn FormatReader>, Tags)> {
    let file = std::fs::File::open(path).with_context(|| format!("open {}", path.display()))?;
    let mss = MediaSourceStream::new(Box::new(file), Default::default());
    let mut hint = Hint::new();
    if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
        hint.with_extension(ext);
    }
    let mut probed = symphonia::default::get_probe()
        .format(&hint, mss, &FormatOptions::default(), &MetadataOptions::default())
        .context("not a supported audio file")?;
    let mut tags = Tags::default();
    if let Some(rev) = probed.metadata.get().as_ref().and_then(|m| m.current()) {
        collect_tags(rev.tags(), &mut tags);
    }
    if let Some(rev) = probed.format.metadata().current() {
        collect_tags(rev.tags(), &mut tags);
    }
    Ok((probed.format, tags))
}

fn collect_tags(src: &[Tag], out: &mut Tags) {
    for tag in src {
        let slot = match tag.std_key {
            Some(StandardTagKey::TrackTitle) => &mut out.title,
            Some(StandardTagKey::Artist) => &mut out.artist,
            Some(StandardTagKey::Album) => &mut out.album,
            Some(StandardTagKey::Lyrics) => &mut out.lyrics,
            _ => continue,
        };
        let value = tag.value.to_string();
        if slot.is_none() && !value.trim().is_empty() {
            *slot = Some(value.trim().to_string());
        }
    }
}

/// The first track that is actually audio (MP4 files may lead with video).
fn audio_track(format: &dyn FormatReader) -> Result<&symphonia::core::formats::Track> {
    format
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL && t.codec_params.sample_rate.is_some())
        .ok_or_else(|| anyhow!("no audio track"))
}

/// Tags and duration (ms), without decoding the audio.
pub fn read_tags(path: &Path) -> Result<(Tags, Option<i64>)> {
    let (format, tags) = open(path)?;
    let p = &audio_track(format.as_ref())?.codec_params;
    let dur = match (p.n_frames, p.sample_rate) {
        (Some(n), Some(rate)) => Some(n as i64 * 1000 / rate as i64),
        _ => None,
    };
    Ok((tags, dur))
}

/// Decode any supported file to 44.1 kHz stereo. Mono is duplicated; channels past two are dropped.
pub fn decode_file(path: &Path) -> Result<Decoded> {
    let (mut format, tags) = open(path)?;
    let track = audio_track(format.as_ref())?;
    let track_id = track.id;
    let rate = track.codec_params.sample_rate.unwrap_or(SAMPLE_RATE);
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("unsupported audio codec")?;
    let (mut left, mut right) = (Vec::new(), Vec::new());
    loop {
        let packet = match format.next_packet() {
            Ok(p) => p,
            Err(SymError::IoError(e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(e.into()),
        };
        if packet.track_id() != track_id {
            continue;
        }
        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(SymError::DecodeError(_)) => continue, // skip a corrupt packet
            Err(e) => return Err(e.into()),
        };
        let spec = *decoded.spec();
        let ch = spec.channels.count();
        let mut buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, spec);
        buf.copy_interleaved_ref(decoded);
        for frame in buf.samples().chunks(ch) {
            left.push(frame[0]);
            right.push(if ch > 1 { frame[1] } else { frame[0] });
        }
    }
    let audio = resample(Stereo { left, right }, rate)?;
    Ok(Decoded { audio, tags })
}

fn resample(a: Stereo, from: u32) -> Result<Stereo> {
    if from == SAMPLE_RATE || a.is_empty() {
        return Ok(a);
    }
    const CHUNK: usize = 1024;
    let mut rs = FftFixedIn::<f32>::new(from as usize, SAMPLE_RATE as usize, CHUNK, 2, 2)?;
    let mut out = [Vec::new(), Vec::new()];
    let mut push = |res: Vec<Vec<f32>>| {
        out[0].extend_from_slice(&res[0]);
        out[1].extend_from_slice(&res[1]);
    };
    let n = a.len();
    let mut pos = 0;
    while pos + CHUNK <= n {
        push(rs.process(&[&a.left[pos..pos + CHUNK], &a.right[pos..pos + CHUNK]], None)?);
        pos += CHUNK;
    }
    if pos < n {
        push(rs.process_partial(Some(&[&a.left[pos..], &a.right[pos..]]), None)?);
    }
    push(rs.process_partial(None::<&[&[f32]]>, None)?);
    let delay = rs.output_delay();
    let expected = (n as u64 * SAMPLE_RATE as u64 / from as u64) as usize;
    let [mut left, mut right] = out;
    for ch in [&mut left, &mut right] {
        ch.drain(..delay.min(ch.len()));
        ch.resize(expected, 0.0);
    }
    Ok(Stereo { left, right })
}

fn to_i16(x: f32) -> i16 {
    (x.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

/// 16-bit stereo FLAC bytes.
pub fn encode_flac(a: &Stereo) -> Result<Vec<u8>> {
    use flacenc::component::BitRepr;
    use flacenc::error::Verify;
    let samples: Vec<i32> = a
        .left
        .iter()
        .zip(&a.right)
        .flat_map(|(l, r)| [to_i16(*l) as i32, to_i16(*r) as i32])
        .collect();
    let config = flacenc::config::Encoder::default()
        .into_verified()
        .map_err(|(_, e)| anyhow!("flac config: {e:?}"))?;
    let source = flacenc::source::MemSource::from_samples(&samples, 2, 16, SAMPLE_RATE as usize);
    let stream = flacenc::encode_with_fixed_block_size(&config, source, config.block_size)
        .map_err(|e| anyhow!("flac encode: {e:?}"))?;
    let mut sink = flacenc::bitsink::ByteSink::new();
    stream.write(&mut sink).map_err(|e| anyhow!("flac write: {e:?}"))?;
    Ok(sink.as_slice().to_vec())
}

pub fn read_flac(path: &Path) -> Result<Stereo> {
    // `encode_flac` pads the final block to a full block size with zeros, so the
    // decoded stream can be longer than the original; STREAMINFO's total-sample
    // count is still exact, so trim back to it.
    let expected = {
        let (format, _) = open(path)?;
        audio_track(format.as_ref())?.codec_params.n_frames.map(|n| n as usize)
    };
    let mut audio = decode_file(path)?.audio;
    if let Some(n) = expected {
        audio.left.truncate(n);
        audio.right.truncate(n);
    }
    Ok(audio)
}

/// SHA-256 (hex) of the audio exactly as it is stored: interleaved 16-bit little-endian.
pub fn audio_hash(a: &Stereo) -> String {
    let mut h = Sha256::new();
    for (l, r) in a.left.iter().zip(&a.right) {
        h.update(to_i16(*l).to_le_bytes());
        h.update(to_i16(*r).to_le_bytes());
    }
    hex::encode(h.finalize())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_util::write_sine_wav;

    fn sine(len: usize) -> Stereo {
        let left: Vec<f32> = (0..len).map(|i| (i as f32 * 0.05).sin() * 0.5).collect();
        let right: Vec<f32> = left.iter().map(|x| -x).collect();
        Stereo { left, right }
    }

    #[test]
    fn decodes_and_resamples_mono_48k_to_stereo_44k() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("mono48.wav");
        write_sine_wav(&p, 48_000, 1, 2.0, 440.0);
        let d = decode_file(&p).unwrap();
        assert_eq!(d.audio.len(), 88_200);
        assert_eq!(d.audio.left, d.audio.right);
        let peak = d.audio.left[20_000..60_000].iter().fold(0f32, |m, x| m.max(x.abs()));
        assert!((0.45..0.55).contains(&peak), "peak {peak}");
    }

    #[test]
    fn six_channel_file_keeps_first_two() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("six.wav");
        write_sine_wav(&p, 44_100, 6, 0.5, 440.0);
        let d = decode_file(&p).unwrap();
        assert_eq!(d.audio.len(), 22_050);
        assert!(d.audio.left.iter().any(|x| x.abs() > 0.4));
    }

    #[test]
    fn read_tags_reports_duration_without_decoding() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("t.wav");
        write_sine_wav(&p, 44_100, 2, 1.5, 440.0);
        let (tags, dur) = read_tags(&p).unwrap();
        assert_eq!(tags, Tags::default());
        assert_eq!(dur, Some(1500));
    }

    #[test]
    fn flac_roundtrip_is_within_one_lsb() {
        let dir = tempfile::tempdir().unwrap();
        let a = sine(44_100);
        let p = dir.path().join("x.flac");
        std::fs::write(&p, encode_flac(&a).unwrap()).unwrap();
        let b = read_flac(&p).unwrap();
        assert_eq!(b.len(), a.len());
        let max = a.left.iter().zip(&b.left).chain(a.right.iter().zip(&b.right))
            .fold(0f32, |m, (x, y)| m.max((x - y).abs()));
        assert!(max <= 1.0 / 32767.0 + 1e-6, "max diff {max}");
    }

    #[test]
    fn hash_is_stable_and_content_sensitive() {
        let a = sine(1000);
        assert_eq!(audio_hash(&a), audio_hash(&a.clone()));
        let mut b = a.clone();
        b.left[500] += 0.01;
        assert_ne!(audio_hash(&a), audio_hash(&b));
        assert_eq!(audio_hash(&a).len(), 64);
    }

    #[test]
    fn rejects_non_audio_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("notes.txt");
        std::fs::write(&p, "hello").unwrap();
        assert!(decode_file(&p).is_err());
    }

    #[test]
    fn stereo_helpers() {
        let mut a = Stereo::silence(10);
        a.append(&Stereo::silence(5));
        assert_eq!(a.len(), 15);
        assert_eq!(a.slice(2, 7).len(), 5);
        assert_eq!(Stereo::silence(44_100).duration_ms(), 1000);
    }
}
