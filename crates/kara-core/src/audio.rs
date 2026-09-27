//! Everything becomes one standard format: 44.1 kHz stereo f32 in memory,
//! 16-bit FLAC for stored vocals. Also tag reading and content hashing.

use anyhow::{anyhow, Context, Result};
use rubato::{FftFixedIn, Resampler};
use sha2::{Digest, Sha256};
use std::path::Path;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{DecoderOptions, CODEC_TYPE_AAC, CODEC_TYPE_NULL};
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
    pub fn len(&self) -> usize {
        self.left.len()
    }
    pub fn is_empty(&self) -> bool {
        self.left.is_empty()
    }
    pub fn duration_ms(&self) -> i64 {
        self.len() as i64 * 1000 / SAMPLE_RATE as i64
    }
}

#[cfg(test)]
impl Stereo {
    pub fn silence(len: usize) -> Self {
        Self { left: vec![0.0; len], right: vec![0.0; len] }
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
    /// From iTunes' gapless tag on AAC audio: encoder delay frames to drop, then frames of real audio.
    pub gapless: Option<(usize, usize)>,
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
        .format(&hint, mss, &FormatOptions { enable_gapless: true, ..Default::default() }, &MetadataOptions::default())
        .context("not a supported audio file")?;
    let mut tags = Tags::default();
    if let Some(rev) = probed.metadata.get().as_ref().and_then(|m| m.current()) {
        collect_tags(rev.tags(), &mut tags);
    }
    if let Some(rev) = probed.format.metadata().current() {
        collect_tags(rev.tags(), &mut tags);
    }
    if !audio_track(probed.format.as_ref()).is_ok_and(|t| t.codec_params.codec == CODEC_TYPE_AAC) {
        tags.gapless = None;
    }
    Ok((probed.format, tags))
}

/// "iTunSMPB" value: " 00000000 <delay> <padding> <length> …" in hex.
fn parse_itunsmpb(value: &str) -> Option<(usize, usize)> {
    let fields: Vec<&str> = value.split_whitespace().collect();
    let hex = |i: usize| usize::from_str_radix(fields.get(i)?, 16).ok();
    Some((hex(1)?, hex(3)?)).filter(|(_, len)| *len > 0)
}

fn collect_tags(src: &[Tag], out: &mut Tags) {
    for tag in src {
        if tag.key.ends_with("iTunSMPB") {
            out.gapless = out.gapless.or(parse_itunsmpb(&tag.value.to_string()));
            continue;
        }
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
    let frames = tags.gapless.map(|(_, len)| len as u64).or(p.n_frames);
    let dur = match (frames, p.sample_rate) {
        (Some(n), Some(rate)) => Some(n as i64 * 1000 / rate as i64),
        _ => None,
    };
    Ok((tags, dur))
}

/// Whether a decode error means the file is damaged, rather than that it couldn't be opened.
pub fn is_damaged(e: &anyhow::Error) -> bool {
    e.downcast_ref::<std::io::Error>().is_none()
}

/// Decode any supported file to 44.1 kHz stereo. Mono is duplicated; channels past two are dropped.
pub fn decode_file(path: &Path) -> Result<Decoded> {
    let mut audio = Stereo::default();
    let tags = decode_stream(path, &mut |l, r| {
        audio.left.extend_from_slice(l);
        audio.right.extend_from_slice(r);
        true
    })?;
    Ok(Decoded { audio, tags })
}

/// Frames `start..start + len` of `decode_file`'s output (fewer at the end of
/// the song), decoding only as far as needed.
pub fn decode_range(path: &Path, start: usize, len: usize) -> Result<Stereo> {
    let (mut out, mut pos) = (Stereo::default(), 0);
    decode_stream(path, &mut |l, r| {
        let lo = start.saturating_sub(pos).min(l.len());
        let hi = (start + len).saturating_sub(pos).min(l.len());
        out.left.extend_from_slice(&l[lo..hi]);
        out.right.extend_from_slice(&r[lo..hi]);
        pos += l.len();
        pos < start + len
    })?;
    Ok(out)
}

type Sink<'a> = &'a mut dyn FnMut(&[f32], &[f32]) -> bool;

/// Decodes to 44.1 kHz stereo, handing `sink` consecutive blocks until it returns false.
fn decode_stream(path: &Path, sink: Sink) -> Result<Tags> {
    let (mut format, tags) = open(path)?;
    let track = audio_track(format.as_ref())?;
    let track_id = track.id;
    let mut out = Output::new(track.codec_params.sample_rate.unwrap_or(SAMPLE_RATE))?;
    let mut decoder = symphonia::default::get_codecs()
        .make(&track.codec_params, &DecoderOptions::default())
        .context("unsupported audio codec")?;
    let (mut skip, mut keep) = tags.gapless.unwrap_or((0, usize::MAX));
    let mut src = [Vec::new(), Vec::new()];
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
        let frames = buf.samples().chunks(ch).skip(skip).take(keep);
        let taken = frames.len();
        for frame in frames {
            src[0].push(frame[0]);
            src[1].push(if ch > 1 { frame[1] } else { frame[0] });
        }
        skip = skip.saturating_sub(buf.samples().len() / ch);
        keep -= taken;
        if !out.push(&mut src, sink)? {
            return Ok(tags);
        }
    }
    out.finish(&src, sink)?;
    Ok(tags)
}

const RESAMPLE_BLOCK: usize = 1024;

/// Turns source-rate audio into 44.1 kHz blocks, the same however far decoding goes.
struct Output {
    resampler: Option<FftFixedIn<f32>>,
    from: u32,
    fed: usize,
    emitter: Emitter,
}

struct Emitter {
    delay_left: usize,
    emitted: usize,
}

impl Emitter {
    /// Hands on `res` minus the resampler's start-up delay, up to `total` frames overall.
    fn emit(&mut self, res: &[Vec<f32>], total: usize, sink: Sink) -> bool {
        let d = self.delay_left.min(res[0].len());
        self.delay_left -= d;
        let n = (res[0].len() - d).min(total.saturating_sub(self.emitted));
        self.emitted += n;
        n == 0 || sink(&res[0][d..d + n], &res[1][d..d + n])
    }
}

impl Output {
    fn new(from: u32) -> Result<Self> {
        let resampler = (from != SAMPLE_RATE)
            .then(|| FftFixedIn::<f32>::new(from as usize, SAMPLE_RATE as usize, RESAMPLE_BLOCK, 2, 2))
            .transpose()?;
        let delay_left = resampler.as_ref().map_or(0, |r| r.output_delay());
        Ok(Self { resampler, from, fed: 0, emitter: Emitter { delay_left, emitted: 0 } })
    }

    /// Converts and hands on every whole block in `src`; false once `sink` wants no more.
    fn push(&mut self, src: &mut [Vec<f32>; 2], sink: Sink) -> Result<bool> {
        let Some(rs) = &mut self.resampler else {
            let more = src[0].is_empty() || sink(&src[0], &src[1]);
            src.iter_mut().for_each(Vec::clear);
            return Ok(more);
        };
        let mut pos = 0;
        while pos + RESAMPLE_BLOCK <= src[0].len() {
            let res = rs.process(&[&src[0][pos..pos + RESAMPLE_BLOCK], &src[1][pos..pos + RESAMPLE_BLOCK]], None)?;
            pos += RESAMPLE_BLOCK;
            self.fed += RESAMPLE_BLOCK;
            if !self.emitter.emit(&res, usize::MAX, sink) {
                return Ok(false);
            }
        }
        src.iter_mut().for_each(|c| drop(c.drain(..pos)));
        Ok(true)
    }

    /// Converts the leftover frames and pads or trims to the exact resampled length.
    fn finish(mut self, src: &[Vec<f32>; 2], sink: Sink) -> Result<()> {
        let Some(mut rs) = self.resampler.take() else { return Ok(()) };
        self.fed += src[0].len();
        let total = (self.fed as u64 * SAMPLE_RATE as u64 / self.from as u64) as usize;
        if !src[0].is_empty() {
            let res = rs.process_partial(Some(&[&src[0][..], &src[1][..]]), None)?;
            self.emitter.emit(&res, total, sink);
        }
        let res = rs.process_partial(None::<&[&[f32]]>, None)?;
        self.emitter.emit(&res, total, sink);
        if self.emitter.emitted < total {
            let silence = vec![0.0; total - self.emitter.emitted];
            sink(&silence, &silence);
        }
        Ok(())
    }
}

fn to_i16(x: f32) -> i16 {
    (x.clamp(-1.0, 1.0) * 32767.0).round() as i16
}

/// 16-bit stereo FLAC bytes.
pub fn encode_flac(a: &Stereo) -> Result<Vec<u8>> {
    use flacenc::component::BitRepr;
    use flacenc::error::Verify;
    let samples: Vec<i32> = a.left.iter().zip(&a.right).flat_map(|(l, r)| [to_i16(*l) as i32, to_i16(*r) as i32]).collect();
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

/// Reads a FLAC file, trimmed to the sample count in its header.
pub fn read_flac(path: &Path) -> Result<Stereo> {
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

/// SHA-256 (hex) of the audio as interleaved 16-bit little-endian samples.
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
        assert_eq!((d.audio.len(), d.audio.duration_ms()), (88_200, 2000));
        assert_eq!(d.audio.left, d.audio.right);
        let peak = d.audio.left[20_000..60_000].iter().fold(0f32, |m, x| m.max(x.abs()));
        assert!((0.45..0.55).contains(&peak), "peak {peak}");
    }

    #[test]
    fn lossy_files_decode_to_their_true_length_and_start() {
        let dir = tempfile::tempdir().unwrap();
        let wav = dir.path().join("s.wav");
        write_sine_wav(&wav, 44_100, 2, 1.0, 330.0);
        let (m4a, mp3, flac) = (dir.path().join("s.m4a"), dir.path().join("s.mp3"), dir.path().join("s.flac"));
        let run = |c: &mut std::process::Command| assert!(c.status().expect("this test needs afconvert and ffmpeg").success());
        run(std::process::Command::new("afconvert").args(["-f", "m4af", "-d", "aac"]).arg(&wav).arg(&m4a));
        // An iTunes gapless tag on a file that isn't AAC must not trim it.
        let tag = "iTunSMPB= 00000000 00000840 000002CE 000000000000AC44";
        run(std::process::Command::new("ffmpeg").args(["-v", "error", "-i"]).arg(&wav).args(["-c:a", "libmp3lame", "-metadata", tag]).arg(&mp3));
        run(std::process::Command::new("ffmpeg").args(["-v", "error", "-i"]).arg(&wav).args(["-metadata", tag]).arg(&flac));
        for p in [m4a, mp3, flac] {
            let a = decode_file(&p).unwrap().audio;
            assert_eq!(a.len(), 44_100, "{}", p.display());
            assert!(a.left[..100].iter().any(|x| x.abs() > 0.1), "{} starts late", p.display());
        }
    }

    #[test]
    fn a_decoded_range_matches_the_same_part_of_the_full_decode() {
        let dir = tempfile::tempdir().unwrap();
        for rate in [44_100, 48_000] {
            let p = dir.path().join(format!("{rate}.wav"));
            write_sine_wav(&p, rate, 2, 2.5, 440.0);
            let full = decode_file(&p).unwrap().audio;
            for (start, len) in [(0, 1000), (44_100, 44_100), (100_000, 44_100), (200_000, 10)] {
                let end = (start + len).min(full.len());
                assert_eq!(decode_range(&p, start, len).unwrap(), full.slice(start.min(end), end), "{rate} Hz, {start}+{len}");
            }
        }
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
    fn flac_is_16_bit_and_roundtrips_within_one_step() {
        let dir = tempfile::tempdir().unwrap();
        let a = sine(44_100);
        let p = dir.path().join("x.flac");
        std::fs::write(&p, encode_flac(&a).unwrap()).unwrap();
        let (format, _) = open(&p).unwrap();
        assert_eq!(audio_track(format.as_ref()).unwrap().codec_params.bits_per_sample, Some(16));
        let b = read_flac(&p).unwrap();
        assert_eq!(b.len(), a.len());
        let max = a.left.iter().zip(&b.left).chain(a.right.iter().zip(&b.right))
            .fold(0f32, |m, (x, y)| m.max((x - y).abs()));
        assert!(max <= 1.0 / 32_767.0, "max diff {max}");
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
}
