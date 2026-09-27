# Phase 1a — Karaoke Engine (kara-core + kara-cli) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A headless Rust engine that takes one file or one link, turns it into the standard audio format, separates vocals from the instrumental in 10-second chunks that stay ahead of playback, finds synced lyrics, keeps a bounded cache, and exposes all of it through a `kara` CLI — including the benchmark that picks the separation model.

**Architecture:** A pure-Rust library crate `kara-core` (no Tauri) holds every stage of the pipeline: `audio` (decode/standardize/FLAC/hash), `separate` (STFT + MDX-Net runner + ONNX Runtime), `library` (SQLite), `lyrics`, `ingest` (files, links, yt-dlp), `cache` (chunk files, eviction, cleanup) and `jobs` (the cancellable worker). A thin `kara-cli` binary drives it for manual use and benchmarks. The Tauri app and Svelte UI (Phase 1b) will call the same crate.

**Tech Stack:** Rust 2021, symphonia 0.5, rubato 0.15, flacenc 0.4, realfft 3, ndarray 0.17, ort =2.0.0-rc.12 (load-dynamic, CoreML), rusqlite 0.31 (bundled, FTS5), reqwest 0.12 (blocking, rustls), clap 4, yt-dlp (downloaded at runtime).

**Spec:** `docs/superpowers/specs/2026-09-26-kara-always-oki-design.md`

**Scope note:** The spec's Phase 1 (MVP) is split in two plans. This plan (1a) is the engine. Plan 1b (Tauri app, Svelte UI, Web Audio streamer) is written after Task 5's model decision, because the chosen model's segment timing feeds the streamer's buffering numbers.

## Global Constraints

- Before any `cargo` command in this environment run: `source "$HOME/.cargo/env"`.
- Target: macOS on Apple Silicon (M1 baseline). Rust edition 2021 (so `std::env::set_var` is a safe call).
- `kara-core` must not depend on Tauri.
- Standard audio format: 44.1 kHz stereo, stored as 16-bit FLAC. Every source is converted to it before separation.
- Chunk files are 10 s: `CHUNK_LEN = 441_000` samples per channel. File names: `NNNN.vocals.flac`, `NNNN.inst.flac`.
- Instrumental = mix − vocals, so vocals + instrumental = original.
- Only external binary: yt-dlp. No ffmpeg.
- ONNX Runtime: `ort = "=2.0.0-rc.12"` with `load-dynamic`; runtime dylib = ONNX Runtime 1.26.0 osx-arm64 from `https://github.com/MohaElder/openenlarge/releases/download/upscaler-assets-v1/libonnxruntime.dylib` (sha256 `ba6ff4015f593fa87682b0e7d36164c1f7fa05148b7dff442efb34e13a60bf1a`). CoreML execution provider on macOS.
- The model session is created once and reused for a whole song.
- Downloads are verified by SHA-256 in memory before writing, then written as `<name>.part` and renamed.
- Memory: separation peak resident memory ≤ 3 GB (user-approved 2026-09-26; leaves ~1 GB of the 4 GB app budget for the UI).
- Cache budget default: 5 GiB (`5 * 1024 * 1024 * 1024` bytes), setting key `cache_budget_bytes`.
- Data root: `~/Library/Application Support/kara-always-oki/` (layout in spec §4).
- Error messages that can reach the UI are plain language with no engineering words (no "stems", "FLAC", "LRCLIB", "chunk", "ONNX").

## Review Focus

1. **Very short audio** (a 50-sample or 2-second clip) — must separate into one chunk without panicking. Test: Task 3 `short_input_makes_one_chunk`.
2. **Odd channel counts and sample rates** (mono 48 kHz, 6-channel) — must become 44.1 kHz stereo; 6-channel keeps the first two channels. Tests: Task 1 `decodes_and_resamples_mono_48k_to_stereo_44k`, `six_channel_file_keeps_first_two`.
3. **Search text with quotes, slashes, accents** (`AC/DC "Back"`, `beyon` → `Beyoncé`) — no SQL/FTS syntax error, accent-insensitive prefix match. Test: Task 6 `search_handles_special_characters_and_accents`.
4. **App quit mid-separation** — leftover `.part` files and a half-done song must resume from the first missing chunk with no corrupt chunk used. Tests: Task 9 `startup_cleanup_removes_parts_and_recounts`, Task 10 `prepare_resumes_after_cancel`.
5. **Same song added twice** — the second copy reuses the first copy's separated audio, no second separation. Test: Task 10 `same_audio_is_separated_once`.

---

## File Structure

```
kara-always-oki/
├─ Cargo.toml                         workspace + profiles
├─ .gitignore
├─ crates/kara-core/
│  ├─ Cargo.toml
│  └─ src/
│     ├─ lib.rs                       module list, now_ms()
│     ├─ test_util.rs                 shared test helpers (cfg(test))
│     ├─ audio.rs                     decode → 44.1k stereo, tags, FLAC, hash
│     ├─ store.rs                     on-disk layout, atomic writes
│     ├─ assets.rs                    download + SHA-256 verify (runtime, model)
│     ├─ separate/mod.rs              CHUNK_LEN, ModelSpec, DEFAULT_MODEL
│     ├─ separate/stft.rs             MDX-compatible STFT / iSTFT
│     ├─ separate/mdx.rs              MdxParams, VocalModel, separate()
│     ├─ separate/onnx.rs             ONNX Runtime-backed VocalModel
│     ├─ library/mod.rs               SQLite API
│     ├─ library/schema.sql           schema v1
│     ├─ lyrics.rs                    LRC parsing, word timing, LRCLIB
│     ├─ ingest/mod.rs                add file / link, fetch audio, collections
│     ├─ ingest/link.rs               link verdicts + user messages
│     ├─ ingest/ytdlp.rs              yt-dlp install + download + title cleanup
│     ├─ cache.rs                     chunk files, finish, eviction, cleanup
│     └─ jobs.rs                      prepare() pipeline + Worker queue
├─ crates/kara-cli/
│  ├─ Cargo.toml
│  ├─ src/main.rs                     add | prepare | search | export | bench
│  └─ tests/cli.rs
└─ docs/superpowers/spikes/2026-09-26-model-choice.md   (Task 5 output)
```

---

### Task 1: Workspace + standard audio (decode, resample, FLAC, hash)

**Files:**
- Create: `Cargo.toml`, `.gitignore`, `crates/kara-core/Cargo.toml`, `crates/kara-core/src/lib.rs`, `crates/kara-core/src/test_util.rs`, `crates/kara-core/src/audio.rs`

**Interfaces:**
- Consumes: nothing.
- Produces:
  - `kara_core::now_ms() -> i64`
  - `kara_core::audio::{SAMPLE_RATE: u32 = 44_100, Stereo, Tags, Decoded}`
  - `Stereo { pub left: Vec<f32>, pub right: Vec<f32> }` with `silence(len)`, `len()`, `is_empty()`, `duration_ms() -> i64`, `slice(start, end)`, `append(&Stereo)`
  - `Tags { title, artist, album, lyrics: Option<String> }`
  - `decode_file(&Path) -> Result<Decoded>` (`Decoded { audio: Stereo, tags: Tags }`)
  - `read_tags(&Path) -> Result<(Tags, Option<i64>)>` (tags, duration in ms, without decoding)
  - `encode_flac(&Stereo) -> Result<Vec<u8>>`, `read_flac(&Path) -> Result<Stereo>`, `audio_hash(&Stereo) -> String`
  - test-only: `test_util::write_sine_wav(path, rate, channels, secs, freq)`

- [ ] **Step 1: Create the workspace files**

`Cargo.toml`:
```toml
[workspace]
resolver = "2"
members = ["crates/kara-core", "crates/kara-cli"]

[workspace.dependencies]
anyhow = "1"
serde = { version = "1", features = ["derive"] }
serde_json = "1"

[profile.release]
lto = "thin"
codegen-units = 1

# ML and DSP code is unusably slow unoptimized; optimize dependencies in dev too.
[profile.dev.package."*"]
opt-level = 3
```

`.gitignore`:
```
/target
```

`crates/kara-core/Cargo.toml`:
```toml
[package]
name = "kara-core"
version = "0.1.0"
edition = "2021"

[dependencies]
anyhow = { workspace = true }
symphonia = { version = "0.5", features = ["all"] }
rubato = "0.15"
flacenc = "0.4"
sha2 = "0.10"
hex = "0.4"

[dev-dependencies]
hound = "3"
tempfile = "3"
```

Create a placeholder CLI crate so the workspace builds (Task 4 fills it in).

`crates/kara-cli/Cargo.toml`:
```toml
[package]
name = "kara-cli"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "kara"
path = "src/main.rs"

[dependencies]
anyhow = { workspace = true }
kara-core = { path = "../kara-core" }
```

`crates/kara-cli/src/main.rs`:
```rust
fn main() {}
```

`crates/kara-core/src/lib.rs`:
```rust
//! kara-core: the karaoke engine. No Tauri here; the app and the CLI both call it.

pub mod audio;

#[cfg(test)]
pub(crate) mod test_util;

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
```

`crates/kara-core/src/test_util.rs`:
```rust
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
```

- [ ] **Step 2: Write the failing tests** — create `crates/kara-core/src/audio.rs` with only the test module:

```rust
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
```

Add `pub mod audio;` is already in lib.rs.

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p kara-core audio`
Expected: compile errors (`decode_file`, `Stereo` … not found).

- [ ] **Step 4: Implement** — put this above the test module in `audio.rs`:

```rust
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
    Ok(decode_file(path)?.audio)
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
```

If `rubato 0.15`'s `process_partial(None, …)` needs a different type annotation on your toolchain, keep the call's meaning (flush with no input) and adjust only the annotation.

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p kara-core audio`
Expected: 7 passed.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml .gitignore crates
git commit -m "feat(core): standard audio format — decode, resample, FLAC, hash"
```

---

### Task 2: MDX-compatible STFT

**Files:**
- Create: `crates/kara-core/src/separate/mod.rs`, `crates/kara-core/src/separate/stft.rs`
- Modify: `crates/kara-core/src/lib.rs` (add `pub mod separate;`), `crates/kara-core/Cargo.toml` (add `realfft = "3"`)

**Interfaces:**
- Produces: `separate::stft::Stft::new(n_fft, hop)`, `.frames(len) -> usize`, `.forward(&[f32]) -> Vec<Vec<Complex32>>` (frames × `n_fft/2+1` bins), `.inverse(&[Vec<Complex32>], len) -> Vec<f32>`; re-export `realfft::num_complex::Complex32`.

This matches `torch.stft(center=True, pad_mode="reflect", window=hann_window(periodic=True))` and `torch.istft`, which is what UVR's MDX-Net models were trained with.

- [ ] **Step 1: Write the failing tests** — `crates/kara-core/src/separate/stft.rs`:

```rust
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
}
```

`crates/kara-core/src/separate/mod.rs`:
```rust
//! Vocal separation: STFT, the MDX-Net runner and the ONNX Runtime model.

pub mod stft;
```

Add `pub mod separate;` to `lib.rs`, and `realfft = "3"` under `[dependencies]` in `crates/kara-core/Cargo.toml`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p kara-core stft`
Expected: compile error (`Stft` not found).

- [ ] **Step 3: Implement** — above the tests in `stft.rs`:

```rust
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
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p kara-core stft`
Expected: 2 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/kara-core
git commit -m "feat(core): MDX-compatible STFT and inverse"
```

---

### Task 3: Chunked MDX separation runner (with fake models)

**Files:**
- Create: `crates/kara-core/src/separate/mdx.rs`
- Modify: `crates/kara-core/src/separate/mod.rs`, `crates/kara-core/src/test_util.rs`, `crates/kara-core/Cargo.toml` (add `ndarray = "0.17"`)

**Interfaces:**
- Consumes: `audio::Stereo`, `separate::stft::{Stft, Complex32}`.
- Produces:
  - `separate::CHUNK_LEN: usize = 441_000`
  - `mdx::MdxParams { n_fft, hop, dim_f, dim_t: usize, compensate: f32 }` with `chunk_size()`, `trim()`, `gen_size()`
  - `mdx::VocalModel: Send { fn infer(&mut self, input: Array4<f32>) -> anyhow::Result<Array4<f32>> }`
  - `mdx::ChunkOut { index: usize, vocals: Stereo, inst: Stereo }`, `mdx::Outcome { Done, Cancelled }`
  - `mdx::chunk_count(total_samples, chunk_len) -> usize`
  - `mdx::separate(model: &mut dyn VocalModel, p: &MdxParams, mix: &Stereo, chunk_len: usize, start_chunk: usize, cancel: &AtomicBool, on_chunk: impl FnMut(ChunkOut) -> Result<()>) -> Result<Outcome>`
  - test-only: `test_util::{test_params() -> MdxParams, Silence, Passthrough}`

How it works: the mix is padded with `trim = n_fft/2` zeros in front. Segment `i` reads `chunk_size = hop·(dim_t−1)` padded samples starting at `i·gen_size`, runs STFT → model → inverse STFT, and keeps the middle `gen_size = chunk_size − 2·trim` samples, which line up with mix samples `[i·gen_size, (i+1)·gen_size)`. Output is regrouped into fixed `chunk_len` chunks. Resuming at chunk `k` restarts at the segment containing sample `k·chunk_len` and skips the samples before it.

- [ ] **Step 1: Add shared test fakes** — append to `test_util.rs`:

```rust
use crate::separate::mdx::{MdxParams, VocalModel};
use ndarray::Array4;

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
```

- [ ] **Step 2: Write the failing tests** — `crates/kara-core/src/separate/mdx.rs`:

```rust
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

    #[test]
    fn passthrough_output_lines_up_with_the_input() {
        let m = mix(2000);
        let chunks = run(&mut Passthrough, &test_params(), &m, 300, 0);
        let v = joined(&chunks, true);
        assert_eq!(v.len(), m.len());
        for k in 100..1900 {
            assert!((v.left[k] - m.left[k]).abs() < 1e-3, "sample {k}: {} vs {}", v.left[k], m.left[k]);
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
```

Update `separate/mod.rs`:
```rust
//! Vocal separation: STFT, the MDX-Net runner and the ONNX Runtime model.

pub mod mdx;
pub mod stft;

/// Samples per channel in one stored chunk file (10 s at 44.1 kHz).
pub const CHUNK_LEN: usize = 441_000;
```

Add `ndarray = "0.17"` to kara-core `[dependencies]`.

- [ ] **Step 3: Run tests to verify they fail**

Run: `cargo test -p kara-core mdx`
Expected: compile errors (`MdxParams`, `separate` not found).

- [ ] **Step 4: Implement** — above the tests in `mdx.rs`:

```rust
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

/// Packs two channels' spectra into the model input. Bins above `dim_f` are
/// dropped and bins 0..3 are zeroed (UVR convention).
pub fn pack(l: &[Vec<Complex32>], r: &[Vec<Complex32>], dim_f: usize) -> Array4<f32> {
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
pub fn unpack(a: &Array4<f32>, n_bins: usize) -> (Vec<Vec<Complex32>>, Vec<Vec<Complex32>>) {
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
```

- [ ] **Step 5: Run tests to verify they pass**

Run: `cargo test -p kara-core mdx`
Expected: 8 passed.

- [ ] **Step 6: Commit**

```bash
git add crates/kara-core
git commit -m "feat(core): chunked MDX separation runner"
```

---

### Task 4: Store layout, verified downloads, ONNX model and `kara bench`

**Files:**
- Create: `crates/kara-core/src/store.rs`, `crates/kara-core/src/assets.rs`, `crates/kara-core/src/separate/onnx.rs`
- Modify: `crates/kara-core/src/lib.rs`, `crates/kara-core/src/separate/mod.rs`, `crates/kara-core/Cargo.toml`, `crates/kara-cli/Cargo.toml`, `crates/kara-cli/src/main.rs`

**Interfaces:**
- Consumes: `mdx::{VocalModel, MdxParams, separate, Outcome}`, `audio::{decode_file, Stereo, SAMPLE_RATE}`, `separate::CHUNK_LEN`.
- Produces:
  - `store::Stem { Vocals, Inst }`, `store::Store::new(root)`, `Store::default_root() -> PathBuf`, `root()`, `db_path()`, `runtime_dir()`, `models_dir()`, `bin_dir()`, `tmp_dir()`, `audio_root()`, `audio_dir(hash)`, `source_path(hash)`, `stems_dir(hash, model_id)`, `chunk_path(hash, model_id, index: u32, Stem)`
  - `store::write_atomic(path: &Path, bytes: &[u8]) -> Result<()>` (writes `<file>.part`, renames)
  - `assets::{Asset { file_name, url, sha256: &'static str }, RUNTIME, sha256_hex(&[u8]) -> String, install(&Asset, dir, bytes) -> Result<PathBuf>, ensure(&Asset, dir, on_progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<PathBuf>}`
  - `separate::onnx::OnnxModel::load(runtime_lib: &Path, model: &Path, use_coreml: bool) -> Result<OnnxModel>`, implements `VocalModel`
  - CLI: `kara bench <input> --model <onnx> [--compensate F] [--cpu] [--runtime <dylib>] [--out <dir>]`

- [ ] **Step 1: Write the failing tests**

`crates/kara-core/src/store.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_matches_the_spec() {
        let s = Store::new("/data");
        assert_eq!(s.db_path(), Path::new("/data/kara.db"));
        assert_eq!(s.source_path("abc"), Path::new("/data/audio/abc/source.flac"));
        assert_eq!(s.chunk_path("abc", "m1", 7, Stem::Vocals), Path::new("/data/audio/abc/m1/0007.vocals.flac"));
        assert_eq!(s.chunk_path("abc", "m1", 12, Stem::Inst), Path::new("/data/audio/abc/m1/0012.inst.flac"));
        assert_eq!(s.bin_dir(), Path::new("/data/bin"));
    }

    #[test]
    fn write_atomic_leaves_no_part_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("a/b/c.bin");
        write_atomic(&p, b"hi").unwrap();
        assert_eq!(std::fs::read(&p).unwrap(), b"hi");
        assert!(!dir.path().join("a/b/c.bin.part").exists());
    }
}
```

`crates/kara-core/src/assets.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    const ABC: Asset = Asset {
        file_name: "abc.bin",
        url: "https://example.invalid/abc.bin",
        sha256: "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
    };

    #[test]
    fn sha256_known_vector() {
        assert_eq!(sha256_hex(b"abc"), ABC.sha256);
    }

    #[test]
    fn install_rejects_a_bad_checksum_and_writes_nothing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(install(&ABC, dir.path(), b"abd").is_err());
        assert_eq!(std::fs::read_dir(dir.path()).unwrap().count(), 0);
    }

    #[test]
    fn install_writes_a_verified_file() {
        let dir = tempfile::tempdir().unwrap();
        let p = install(&ABC, dir.path(), b"abc").unwrap();
        assert_eq!(std::fs::read(p).unwrap(), b"abc");
    }

    #[test]
    fn ensure_skips_download_when_file_is_valid() {
        let dir = tempfile::tempdir().unwrap();
        install(&ABC, dir.path(), b"abc").unwrap();
        // The URL is unreachable, so this only passes if no download is attempted.
        let p = ensure(&ABC, dir.path(), &mut |_, _| {}).unwrap();
        assert_eq!(std::fs::read(p).unwrap(), b"abc");
    }
}
```

Update `lib.rs` module list:
```rust
pub mod assets;
pub mod audio;
pub mod separate;
pub mod store;
```

Add to `separate/mod.rs`: `pub mod onnx;`

Add to kara-core `[dependencies]`:
```toml
dirs = "5"
reqwest = { version = "0.12", default-features = false, features = ["blocking", "json", "rustls-tls"] }
ort = { version = "=2.0.0-rc.12", default-features = false, features = ["load-dynamic", "ndarray", "api-24"] }

[target.'cfg(target_os = "macos")'.dependencies]
ort = { version = "=2.0.0-rc.12", default-features = false, features = ["load-dynamic", "ndarray", "api-24", "coreml"] }
```
(Place the `[target…]` table after `[dependencies]` and before `[dev-dependencies]`.)

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p kara-core store assets`
(Cargo takes one filter; run `cargo test -p kara-core store` then `cargo test -p kara-core assets`.)
Expected: compile errors (`Store`, `Asset` not found).

- [ ] **Step 3: Implement `store.rs`** (above its tests):

```rust
//! Where everything lives on disk (spec §4).

use anyhow::Result;
use std::path::{Path, PathBuf};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stem {
    Vocals,
    Inst,
}

impl Stem {
    fn tag(self) -> &'static str {
        match self {
            Stem::Vocals => "vocals",
            Stem::Inst => "inst",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Store {
    root: PathBuf,
}

impl Store {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }
    /// `~/Library/Application Support/kara-always-oki`
    pub fn default_root() -> PathBuf {
        dirs::data_dir().expect("no user data directory").join("kara-always-oki")
    }
    pub fn root(&self) -> &Path {
        &self.root
    }
    pub fn db_path(&self) -> PathBuf {
        self.root.join("kara.db")
    }
    pub fn runtime_dir(&self) -> PathBuf {
        self.root.join("runtime")
    }
    pub fn models_dir(&self) -> PathBuf {
        self.root.join("models")
    }
    pub fn bin_dir(&self) -> PathBuf {
        self.root.join("bin")
    }
    pub fn tmp_dir(&self) -> PathBuf {
        self.root.join("tmp")
    }
    pub fn audio_root(&self) -> PathBuf {
        self.root.join("audio")
    }
    pub fn audio_dir(&self, hash: &str) -> PathBuf {
        self.audio_root().join(hash)
    }
    pub fn source_path(&self, hash: &str) -> PathBuf {
        self.audio_dir(hash).join("source.flac")
    }
    pub fn stems_dir(&self, hash: &str, model_id: &str) -> PathBuf {
        self.audio_dir(hash).join(model_id)
    }
    pub fn chunk_path(&self, hash: &str, model_id: &str, index: u32, stem: Stem) -> PathBuf {
        self.stems_dir(hash, model_id).join(format!("{index:04}.{}.flac", stem.tag()))
    }
}

/// Writes `<path>.part`, then renames it into place, so readers never see a half-written file.
pub fn write_atomic(path: &Path, bytes: &[u8]) -> Result<()> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    let mut part = path.as_os_str().to_owned();
    part.push(".part");
    std::fs::write(&part, bytes)?;
    std::fs::rename(&part, path)?;
    Ok(())
}
```

- [ ] **Step 4: Implement `assets.rs`** (above its tests):

```rust
//! Files downloaded on first use, verified by SHA-256 before anything is written.

use crate::store::write_atomic;
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::path::{Path, PathBuf};

pub struct Asset {
    pub file_name: &'static str,
    pub url: &'static str,
    pub sha256: &'static str,
}

/// ONNX Runtime 1.26.0 osx-arm64, re-signed for hardened runtime (hosted with OpenEnlarge's assets).
pub const RUNTIME: Asset = Asset {
    file_name: "libonnxruntime.dylib",
    url: "https://github.com/MohaElder/openenlarge/releases/download/upscaler-assets-v1/libonnxruntime.dylib",
    sha256: "ba6ff4015f593fa87682b0e7d36164c1f7fa05148b7dff442efb34e13a60bf1a",
};

pub fn sha256_hex(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}

/// Verifies `bytes` against the manifest, then writes them into `dir`.
pub fn install(asset: &Asset, dir: &Path, bytes: &[u8]) -> Result<PathBuf> {
    let got = sha256_hex(bytes);
    if got != asset.sha256 {
        bail!("{} failed its checksum (got {got})", asset.file_name);
    }
    let path = dir.join(asset.file_name);
    write_atomic(&path, bytes)?;
    Ok(path)
}

/// Path to a verified copy of `asset` in `dir`, downloading it if missing or corrupt.
pub fn ensure(asset: &Asset, dir: &Path, on_progress: &mut dyn FnMut(u64, Option<u64>)) -> Result<PathBuf> {
    let path = dir.join(asset.file_name);
    if let Ok(bytes) = std::fs::read(&path) {
        if sha256_hex(&bytes) == asset.sha256 {
            return Ok(path);
        }
    }
    let mut resp = reqwest::blocking::get(asset.url)
        .and_then(|r| r.error_for_status())
        .with_context(|| format!("download {}", asset.url))?;
    let total = resp.content_length();
    let mut bytes = Vec::with_capacity(total.unwrap_or(0) as usize);
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = resp.read(&mut buf)?;
        if n == 0 {
            break;
        }
        bytes.extend_from_slice(&buf[..n]);
        on_progress(bytes.len() as u64, total);
    }
    install(asset, dir, &bytes)
}
```

- [ ] **Step 5: Run the store and asset tests**

Run: `cargo test -p kara-core store` then `cargo test -p kara-core assets`
Expected: 2 passed, then 4 passed.

- [ ] **Step 6: Implement `separate/onnx.rs`** (exercised by `kara bench` in Task 5; no unit test because it needs the real runtime and model):

```rust
//! ONNX Runtime-backed vocal model. One session per song, reused for every segment.

use super::mdx::VocalModel;
use anyhow::{anyhow, Result};
use ndarray::{Array4, Ix4};
use ort::session::{builder::GraphOptimizationLevel, Session};
use ort::value::Tensor;
use std::path::Path;
use std::sync::Once;

static INIT: Once = Once::new();

pub struct OnnxModel {
    session: Session,
    input: String,
    output: String,
}

impl OnnxModel {
    /// `runtime_lib` is the downloaded ONNX Runtime dylib (see `assets::RUNTIME`).
    pub fn load(runtime_lib: &Path, model: &Path, use_coreml: bool) -> Result<Self> {
        let lib = runtime_lib.to_path_buf();
        INIT.call_once(|| std::env::set_var("ORT_DYLIB_PATH", lib));
        let mut builder = Session::builder()
            .map_err(|e| anyhow!("{e}"))?
            .with_optimization_level(GraphOptimizationLevel::Level3)
            .map_err(|e| anyhow!("{e}"))?;
        #[cfg(target_os = "macos")]
        if use_coreml {
            use ort::execution_providers::CoreMLExecutionProvider;
            builder = builder
                .with_execution_providers([CoreMLExecutionProvider::default().build()])
                .map_err(|e| anyhow!("{e}"))?;
        }
        #[cfg(not(target_os = "macos"))]
        let _ = use_coreml;
        let session = builder.commit_from_file(model).map_err(|e| anyhow!("load model: {e}"))?;
        // Community conversions name their tensors differently; read the names from the model.
        let input = session.inputs()[0].name().to_string();
        let output = session.outputs()[0].name().to_string();
        Ok(Self { session, input, output })
    }
}

impl VocalModel for OnnxModel {
    fn infer(&mut self, input: Array4<f32>) -> Result<Array4<f32>> {
        let tensor = Tensor::from_array(input).map_err(|e| anyhow!("{e}"))?;
        let outputs = self
            .session
            .run(ort::inputs![self.input.as_str() => tensor])
            .map_err(|e| anyhow!("inference: {e}"))?;
        let view = outputs[self.output.as_str()]
            .try_extract_array::<f32>()
            .map_err(|e| anyhow!("{e}"))?;
        Ok(view.to_owned().into_dimensionality::<Ix4>()?)
    }
}
```

- [ ] **Step 7: Implement `kara bench`**

`crates/kara-cli/Cargo.toml` dependencies:
```toml
[dependencies]
anyhow = { workspace = true }
kara-core = { path = "../kara-core" }
clap = { version = "4", features = ["derive"] }
hound = "3"
libc = "0.2"
```

`crates/kara-cli/src/main.rs`:
```rust
use anyhow::Result;
use clap::{Parser, Subcommand};
use kara_core::audio::{self, Stereo, SAMPLE_RATE};
use kara_core::separate::mdx::{self, MdxParams};
use kara_core::separate::onnx::OnnxModel;
use kara_core::separate::CHUNK_LEN;
use kara_core::store::Store;
use kara_core::assets;
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicBool;
use std::time::Instant;

#[derive(Parser)]
#[command(name = "kara", about = "kara-always-oki engine")]
struct Cli {
    /// Data folder (defaults to ~/Library/Application Support/kara-always-oki)
    #[arg(long, global = true)]
    data: Option<PathBuf>,
    #[command(subcommand)]
    cmd: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
    /// Measure separation speed and memory for one model on one song.
    Bench {
        input: PathBuf,
        #[arg(long)]
        model: PathBuf,
        #[arg(long, default_value_t = 1.0)]
        compensate: f32,
        /// Run on CPU instead of CoreML.
        #[arg(long)]
        cpu: bool,
        /// Use this ONNX Runtime dylib instead of downloading one.
        #[arg(long)]
        runtime: Option<PathBuf>,
        /// Write vocals.wav and instrumental.wav here for a listening test.
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();
    let store = Store::new(cli.data.clone().unwrap_or_else(Store::default_root));
    match cli.cmd {
        Cmd::Bench { input, model, compensate, cpu, runtime, out } => {
            bench(&store, &input, &model, compensate, !cpu, runtime, out.as_deref())
        }
    }
}

pub(crate) fn runtime_lib(store: &Store, over: Option<PathBuf>) -> Result<PathBuf> {
    match over {
        Some(p) => Ok(p),
        None => assets::ensure(&assets::RUNTIME, &store.runtime_dir(), &mut |done, total| {
            if let Some(t) = total {
                eprint!("\rDownloading ONNX Runtime… {}%", done * 100 / t.max(1));
            }
        }),
    }
}

fn peak_rss_mb() -> f64 {
    // macOS reports ru_maxrss in bytes.
    let mut u: libc::rusage = unsafe { std::mem::zeroed() };
    unsafe { libc::getrusage(libc::RUSAGE_SELF, &mut u) };
    u.ru_maxrss as f64 / (1024.0 * 1024.0)
}

fn write_wav(path: &Path, a: &Stereo) -> Result<()> {
    let spec = hound::WavSpec { channels: 2, sample_rate: SAMPLE_RATE, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(path, spec)?;
    for (l, r) in a.left.iter().zip(&a.right) {
        w.write_sample((l.clamp(-1.0, 1.0) * 32767.0) as i16)?;
        w.write_sample((r.clamp(-1.0, 1.0) * 32767.0) as i16)?;
    }
    w.finalize()?;
    Ok(())
}

fn bench(store: &Store, input: &Path, model_path: &Path, compensate: f32, coreml: bool, runtime: Option<PathBuf>, out: Option<&Path>) -> Result<()> {
    let lib = runtime_lib(store, runtime)?;
    let t0 = Instant::now();
    let mix = audio::decode_file(input)?.audio;
    let decode_s = t0.elapsed().as_secs_f64();
    let t1 = Instant::now();
    let mut model = OnnxModel::load(&lib, model_path, coreml)?;
    let load_s = t1.elapsed().as_secs_f64();
    let params = MdxParams { n_fft: 7680, hop: 1024, dim_f: 3072, dim_t: 256, compensate };
    let (mut vocals, mut inst) = (Stereo::default(), Stereo::default());
    let mut first_chunk_s = None;
    let t2 = Instant::now();
    mdx::separate(&mut model, &params, &mix, CHUNK_LEN, 0, &AtomicBool::new(false), |c| {
        first_chunk_s.get_or_insert(t2.elapsed().as_secs_f64());
        vocals.append(&c.vocals);
        inst.append(&c.inst);
        Ok(())
    })?;
    let sep_s = t2.elapsed().as_secs_f64();
    let audio_s = mix.len() as f64 / SAMPLE_RATE as f64;
    println!("song             {:.1} s", audio_s);
    println!("decode           {:.2} s", decode_s);
    println!("model load       {:.2} s  ({})", load_s, if coreml { "CoreML" } else { "CPU" });
    println!("first 10 s chunk {:.2} s", first_chunk_s.unwrap_or(0.0));
    println!("separation       {:.1} s  = {:.2}x real time", sep_s, audio_s / sep_s);
    println!("peak memory      {:.0} MB", peak_rss_mb());
    if let Some(dir) = out {
        std::fs::create_dir_all(dir)?;
        write_wav(&dir.join("vocals.wav"), &vocals)?;
        write_wav(&dir.join("instrumental.wav"), &inst)?;
        println!("wrote {}", dir.display());
    }
    Ok(())
}
```

- [ ] **Step 8: Build and run all tests**

Run: `cargo build -p kara-cli && cargo test -p kara-core`
Expected: builds; all tests pass (Tasks 1–4).

- [ ] **Step 9: Commit**

```bash
git add crates
git commit -m "feat: store layout, verified asset downloads, ONNX model, kara bench"
```

---

### Task 5: SPIKE — choose the separation model

This task produces a decision document and one constant. Nothing ships without it.

**Files:**
- Create: `docs/superpowers/spikes/2026-09-26-model-choice.md`
- Modify: `crates/kara-core/src/separate/mod.rs` (add `ModelSpec`, `DEFAULT_MODEL`)

**Interfaces:**
- Consumes: `kara bench` (Task 4), `assets::Asset`, `mdx::MdxParams`.
- Produces: `separate::ModelSpec { id: &'static str, asset: Asset, params: MdxParams }`, `separate::DEFAULT_MODEL: ModelSpec`.

Candidates (both MDX-Net vocal models from UVR, same pipeline, n_fft 7680 / dim_f 3072 / dim_t 256 / hop 1024):
- `Kim_Vocal_2.onnx` — `https://github.com/TRvlvr/model_repo/releases/download/all_public_uvr_models/Kim_Vocal_2.onnx`
- `UVR-MDX-NET-Voc_FT.onnx` — `https://github.com/TRvlvr/model_repo/releases/download/all_public_uvr_models/UVR-MDX-NET-Voc_FT.onnx`

HTDemucs is only benchmarked if neither candidate passes the gate (it needs a different, heavier pipeline).

- [ ] **Step 1: Download both candidates**

```bash
M="$HOME/Library/Application Support/kara-always-oki/models"; mkdir -p "$M"
curl -L -o "$M/Kim_Vocal_2.onnx" https://github.com/TRvlvr/model_repo/releases/download/all_public_uvr_models/Kim_Vocal_2.onnx
curl -L -o "$M/UVR-MDX-NET-Voc_FT.onnx" https://github.com/TRvlvr/model_repo/releases/download/all_public_uvr_models/UVR-MDX-NET-Voc_FT.onnx
shasum -a 256 "$M"/*.onnx
```
Expected: two files of tens of MB and two SHA-256 lines. Record them.

- [ ] **Step 2: Look up each model's parameters in UVR's model data**

UVR identifies a model by the MD5 of its last 10,000 KiB:
```bash
for f in "$M"/*.onnx; do
  h=$(python3 -c "import hashlib,sys;f=open(sys.argv[1],'rb');f.seek(-10000*1024,2);print(hashlib.md5(f.read()).hexdigest())" "$f")
  echo "$(basename "$f") $h"
  curl -s https://raw.githubusercontent.com/TRvlvr/application_data/main/mdx_model_data/model_data_new.json \
    | python3 -c "import json,sys;print(json.load(sys.stdin).get('$h'))"
done
```
Expected: each prints a dict with `mdx_dim_f_set: 3072`, `mdx_dim_t_set: 8`, `mdx_n_fft_scale_set: 7680`, `primary_stem: 'Vocals'` and a `compensate` value. If a model's values differ from 3072 / 8 / 7680, stop and report — the bench hard-codes those.

- [ ] **Step 3: Check the licenses**

Read the README / release notes of `TRvlvr/model_repo` and the UVR project for the terms these model files are distributed under. Record what you find (or that it's unstated) in the spike doc. An unstated license is a finding to raise with the user, not a blocker for the benchmark.

- [ ] **Step 4: Benchmark**

Pick three of your own songs of 3–5 minutes (one dense rock/pop, one sparse ballad, one with heavy backing vocals). For each song and each model, run with CoreML, then once with `--cpu` for the fastest model:

```bash
cargo run --release -p kara-cli -- bench "<song>" --model "$M/Kim_Vocal_2.onnx" --compensate <value from step 2> --out /tmp/kara-bench/kim/<song-name>
cargo run --release -p kara-cli -- bench "<song>" --model "$M/UVR-MDX-NET-Voc_FT.onnx" --compensate <value from step 2> --out /tmp/kara-bench/vocft/<song-name>
```
Expected: printed report per run. If CoreML fails to load or is slower than CPU, note it.

- [ ] **Step 5: Listen**

Play each `instrumental.wav`. Rate vocal residue and artifacts 1–5 per song per model.

- [ ] **Step 6: Write the spike document** — `docs/superpowers/spikes/2026-09-26-model-choice.md`:

```markdown
# Model choice — spike results

Machine: <model, chip, RAM, macOS version>

| Model | Song | Length | x real time (CoreML) | x real time (CPU) | First 10 s chunk | Peak MB | Quality 1–5 |
|---|---|---|---|---|---|---|---|
| Kim_Vocal_2 | … | … | … | … | … | … | … |
| Voc_FT | … | … | … | … | … | … | … |

Parameters (from UVR model data): <model>: n_fft 7680, dim_f 3072, dim_t 256 (2^8), hop 1024, compensate <value>
SHA-256: <model>: <hash>
License: <findings>

Gate: ≥ 2× real time and ≤ 1500 MB peak on this machine.
Decision: <model> because <reason>.
```

**Gate:** the chosen model must be ≥ 2× real time and ≤ 1500 MB peak memory. If neither passes, stop here and report the numbers to the user with the options (CPU vs CoreML, HTDemucs, smaller `dim_t`); do not continue to Task 6.

- [ ] **Step 7: Add the chosen model as the default** — append to `crates/kara-core/src/separate/mod.rs`, filling the three recorded values (file name/URL from the candidate list, SHA-256 from Step 1, compensate from Step 2):

```rust
use crate::assets::Asset;
use mdx::MdxParams;

pub struct ModelSpec {
    /// Folder name for this model's chunk files, e.g. "kim-vocal-2".
    pub id: &'static str,
    pub asset: Asset,
    pub params: MdxParams,
}

/// Chosen in docs/superpowers/spikes/2026-09-26-model-choice.md.
pub const DEFAULT_MODEL: ModelSpec = ModelSpec {
    id: "kim-vocal-2", // or "voc-ft"
    asset: Asset {
        file_name: "Kim_Vocal_2.onnx", // or "UVR-MDX-NET-Voc_FT.onnx"
        url: "https://github.com/TRvlvr/model_repo/releases/download/all_public_uvr_models/Kim_Vocal_2.onnx",
        sha256: "<SHA-256 recorded in the spike doc>",
    },
    params: MdxParams { n_fft: 7680, hop: 1024, dim_f: 3072, dim_t: 256, compensate: 1.0 /* recorded value */ },
};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_model_is_consistent() {
        let p = DEFAULT_MODEL.params;
        assert_eq!(p.chunk_size(), 261_120);
        assert_eq!(p.gen_size(), 253_440);
        assert_eq!(DEFAULT_MODEL.asset.sha256.len(), 64);
        assert!(p.compensate > 0.9 && p.compensate < 1.2);
    }
}
```

- [ ] **Step 8: Run tests**

Run: `cargo test -p kara-core default_model`
Expected: 1 passed.

- [ ] **Step 9: Commit**

```bash
git add docs/superpowers/spikes crates/kara-core/src/separate/mod.rs
git commit -m "spike: choose separation model; add DEFAULT_MODEL"
```

---

### Task 6: Library database (SQLite)

**Files:**
- Create: `crates/kara-core/src/library/mod.rs`, `crates/kara-core/src/library/schema.sql`
- Modify: `crates/kara-core/src/lib.rs` (add `pub mod library;` and `pub mod lyrics;`), `crates/kara-core/Cargo.toml`

The library stores `lyrics::Line` values, so this task also creates the `Line`/`Word` types in `lyrics.rs` (Task 7 adds the parsing).

**Interfaces:**
- Consumes: `now_ms()`.
- Produces (all in `kara_core::library`):
  - enums (each with `as_str()`, `FromStr`, SQL conversion): `ProviderId { Local, Spotify, Apple }`, `CollectionKind { Playlist, Album, Artist }`, `SourceKind { File, Link, Match }`, `SourceStatus { Pending, Fetching, Ready, Failed }`, `SepStatus { Queued, Running, Ready, Failed, Cancelled }`, `LyricsSource { Lrclib, Embedded, None }`
  - `NewTrack<'a> { provider: ProviderId, provider_ref: Option<&'a str>, title: &'a str, artist: Option<&'a str>, album: Option<&'a str>, duration_ms: Option<i64> }`
  - `Track { id: i64, provider: ProviderId, title: String, artist: Option<String>, album: Option<String>, duration_ms: Option<i64>, vocal_removal: u8, key_semitones: i8 }`
  - `CollectionRow { id: i64, provider: ProviderId, kind: CollectionKind, name: String, subtitle: Option<String> }`
  - `AudioSource { id, track_id: i64, kind: SourceKind, uri: String, label: Option<String>, audio_hash: Option<String>, lyric_offset_ms: i64, status: SourceStatus, error: Option<String> }`
  - `SeparationRow { audio_hash: String, model_id: String, chunk_ms: u32, chunks_total: u32, chunks_done: u32, status: SepStatus, size_bytes: i64, last_used_at: Option<i64> }`
  - `LyricsRow { source: LyricsSource, lines: Vec<lyrics::Line>, fetched_at: i64 }`
  - `Library::{open(&Path), open_in_memory(), add_track(&NewTrack) -> i64, track(id) -> Track, update_track_meta(id, title, artist: Option<&str>, album: Option<&str>), set_track_settings(id, vocal_removal: u8, key: i8), upsert_collection(provider, kind, provider_ref: &str, name, subtitle: Option<&str>) -> i64, add_to_collection(collection_id, track_id), collections(Option<ProviderId>, CollectionKind) -> Vec<CollectionRow>, collection_tracks(id) -> Vec<Track>, add_source(track_id, SourceKind, uri, label: Option<&str>) -> i64, selected_source(track_id) -> Option<AudioSource>, sources_with_hash(hash) -> Vec<AudioSource>, set_source_status(id, SourceStatus, error: Option<&str>), set_source_audio(id, hash, duration_ms), reset_sources_for_hash(hash), set_lyric_offset(source_id, ms), search(query, limit: u32) -> Vec<Track>, separation(hash, model_id) -> Option<SeparationRow>, separations() -> Vec<SeparationRow>, upsert_separation(&SeparationRow), touch_separation(hash, model_id, at_ms), delete_separation(hash, model_id), lyrics(track_id) -> Option<LyricsRow>, set_lyrics(track_id, LyricsSource, &[Line], at_ms), setting(key) -> Option<String>, set_setting(key, value)}` — all return `anyhow::Result`.
  - `lyrics::{Line { start_ms, end_ms: i64, text: String, words: Vec<Word> }, Word { start_ms, end_ms: i64, text: String }}` (serde)

- [ ] **Step 1: Schema** — `crates/kara-core/src/library/schema.sql` (spec §4 plus FTS sync triggers and CHECKs):

```sql
CREATE TABLE provider_account (
  provider     TEXT PRIMARY KEY CHECK (provider IN ('spotify','apple')),
  display_name TEXT,
  connected_at INTEGER
);

CREATE TABLE track (
  id            INTEGER PRIMARY KEY,
  provider      TEXT NOT NULL CHECK (provider IN ('local','spotify','apple')),
  provider_ref  TEXT,
  title         TEXT NOT NULL,
  artist        TEXT,
  album         TEXT,
  duration_ms   INTEGER,
  artwork_path  TEXT,
  added_at      INTEGER NOT NULL,
  vocal_removal INTEGER NOT NULL DEFAULT 100 CHECK (vocal_removal BETWEEN 0 AND 100),
  key_semitones INTEGER NOT NULL DEFAULT 0 CHECK (key_semitones BETWEEN -6 AND 6),
  UNIQUE (provider, provider_ref)
);

CREATE TABLE collection (
  id           INTEGER PRIMARY KEY,
  provider     TEXT NOT NULL CHECK (provider IN ('local','spotify','apple')),
  kind         TEXT NOT NULL CHECK (kind IN ('playlist','album','artist')),
  provider_ref TEXT NOT NULL,
  name         TEXT NOT NULL,
  subtitle     TEXT,
  artwork_path TEXT,
  UNIQUE (provider, kind, provider_ref)
);

CREATE TABLE collection_track (
  collection_id INTEGER NOT NULL REFERENCES collection ON DELETE CASCADE,
  track_id      INTEGER NOT NULL REFERENCES track ON DELETE CASCADE,
  position      INTEGER NOT NULL,
  PRIMARY KEY (collection_id, track_id)
);

CREATE TABLE audio_source (
  id              INTEGER PRIMARY KEY,
  track_id        INTEGER NOT NULL REFERENCES track ON DELETE CASCADE,
  kind            TEXT NOT NULL CHECK (kind IN ('file','link','match')),
  uri             TEXT NOT NULL,
  label           TEXT,
  duration_ms     INTEGER,
  selected        INTEGER NOT NULL DEFAULT 0,
  audio_hash      TEXT,
  lyric_offset_ms INTEGER NOT NULL DEFAULT 0,
  status          TEXT NOT NULL CHECK (status IN ('pending','fetching','ready','failed')),
  error           TEXT
);
CREATE UNIQUE INDEX one_selected_source ON audio_source(track_id) WHERE selected = 1;
CREATE INDEX audio_source_hash ON audio_source(audio_hash);

CREATE TABLE separation (
  audio_hash   TEXT NOT NULL,
  model_id     TEXT NOT NULL,
  chunk_ms     INTEGER NOT NULL,
  chunks_total INTEGER NOT NULL,
  chunks_done  INTEGER NOT NULL DEFAULT 0,
  status       TEXT NOT NULL CHECK (status IN ('queued','running','ready','failed','cancelled')),
  size_bytes   INTEGER NOT NULL DEFAULT 0,
  last_used_at INTEGER,
  PRIMARY KEY (audio_hash, model_id)
);

CREATE TABLE lyrics (
  track_id   INTEGER PRIMARY KEY REFERENCES track ON DELETE CASCADE,
  source     TEXT NOT NULL CHECK (source IN ('lrclib','embedded','none')),
  lines      TEXT,
  fetched_at INTEGER NOT NULL
);

CREATE TABLE setting (key TEXT PRIMARY KEY, value TEXT);

CREATE VIRTUAL TABLE track_fts USING fts5(
  title, artist, album, content='track', content_rowid='id',
  tokenize = 'unicode61 remove_diacritics 2'
);
CREATE TRIGGER track_ai AFTER INSERT ON track BEGIN
  INSERT INTO track_fts(rowid, title, artist, album) VALUES (new.id, new.title, new.artist, new.album);
END;
CREATE TRIGGER track_ad AFTER DELETE ON track BEGIN
  INSERT INTO track_fts(track_fts, rowid, title, artist, album) VALUES ('delete', old.id, old.title, old.artist, old.album);
END;
CREATE TRIGGER track_au AFTER UPDATE OF title, artist, album ON track BEGIN
  INSERT INTO track_fts(track_fts, rowid, title, artist, album) VALUES ('delete', old.id, old.title, old.artist, old.album);
  INSERT INTO track_fts(rowid, title, artist, album) VALUES (new.id, new.title, new.artist, new.album);
END;
```

Note: `collection.provider_ref` is `NOT NULL` here (spec said nullable) because SQLite treats NULLs as distinct in UNIQUE; Local collections use name-based keys like `album:<artist>:<album>` so they de-duplicate.

- [ ] **Step 2: Lyrics types** — create `crates/kara-core/src/lyrics.rs`:

```rust
//! Synced lyrics: types, LRC parsing, word timing and lookup.

use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Word {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub start_ms: i64,
    pub end_ms: i64,
    pub text: String,
    pub words: Vec<Word>,
}
```

Add to kara-core `[dependencies]`:
```toml
rusqlite = { version = "0.31", features = ["bundled"] }
serde = { workspace = true }
serde_json = { workspace = true }
```
Add `pub mod library;` and `pub mod lyrics;` to `lib.rs`.

- [ ] **Step 3: Write the failing tests** — `crates/kara-core/src/library/mod.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::lyrics::{Line, Word};

    fn lib() -> Library {
        Library::open_in_memory().unwrap()
    }

    fn local<'a>(title: &'a str, artist: Option<&'a str>) -> NewTrack<'a> {
        NewTrack { provider: ProviderId::Local, provider_ref: None, title, artist, album: None, duration_ms: Some(1000) }
    }

    #[test]
    fn adds_and_reads_a_track_with_default_settings() {
        let l = lib();
        let id = l.add_track(&local("Paper Satellites", Some("Mina Okada"))).unwrap();
        let t = l.track(id).unwrap();
        assert_eq!((t.title.as_str(), t.artist.as_deref(), t.vocal_removal, t.key_semitones), ("Paper Satellites", Some("Mina Okada"), 100, 0));
        l.set_track_settings(id, 40, -2).unwrap();
        let t = l.track(id).unwrap();
        assert_eq!((t.vocal_removal, t.key_semitones), (40, -2));
    }

    #[test]
    fn rejects_out_of_range_settings() {
        let l = lib();
        let id = l.add_track(&local("x", None)).unwrap();
        assert!(l.set_track_settings(id, 100, 7).is_err());
    }

    #[test]
    fn search_handles_special_characters_and_accents() {
        let l = lib();
        l.add_track(&local("Crazy in Love", Some("Beyoncé"))).unwrap();
        l.add_track(&local("Back in Black", Some("AC/DC"))).unwrap();
        assert_eq!(l.search("beyon", 10).unwrap()[0].title, "Crazy in Love");
        assert_eq!(l.search("back bla", 10).unwrap()[0].artist.as_deref(), Some("AC/DC"));
        assert!(l.search("AC/DC \"Back\" *", 10).is_ok());
        assert!(l.search("   ", 10).unwrap().is_empty());
    }

    #[test]
    fn search_sees_renamed_tracks() {
        let l = lib();
        let id = l.add_track(&local("youtube.com link", None)).unwrap();
        l.update_track_meta(id, "Midnight Laundromat", Some("Kiko"), None).unwrap();
        assert_eq!(l.search("laundro", 10).unwrap().len(), 1);
        assert!(l.search("youtube", 10).unwrap().is_empty());
    }

    #[test]
    fn collections_filter_by_provider_and_keep_order() {
        let l = lib();
        let a = l.add_track(&local("A", None)).unwrap();
        let b = l.add_track(&local("B", None)).unwrap();
        let imported = l.upsert_collection(ProviderId::Local, CollectionKind::Playlist, "imported", "Imported", None).unwrap();
        assert_eq!(imported, l.upsert_collection(ProviderId::Local, CollectionKind::Playlist, "imported", "Imported", None).unwrap());
        l.add_to_collection(imported, b).unwrap();
        l.add_to_collection(imported, a).unwrap();
        l.add_to_collection(imported, b).unwrap(); // duplicate ignored
        let titles: Vec<_> = l.collection_tracks(imported).unwrap().into_iter().map(|t| t.title).collect();
        assert_eq!(titles, vec!["B", "A"]);
        assert_eq!(l.collections(None, CollectionKind::Playlist).unwrap().len(), 1);
        assert_eq!(l.collections(Some(ProviderId::Spotify), CollectionKind::Playlist).unwrap().len(), 0);
        assert_eq!(l.collections(Some(ProviderId::Local), CollectionKind::Album).unwrap().len(), 0);
    }

    #[test]
    fn first_source_is_selected_and_audio_can_be_set() {
        let l = lib();
        let t = l.add_track(&local("A", None)).unwrap();
        let s1 = l.add_source(t, SourceKind::File, "/music/a.mp3", Some("a.mp3")).unwrap();
        l.add_source(t, SourceKind::Link, "https://youtu.be/x", None).unwrap();
        let sel = l.selected_source(t).unwrap().unwrap();
        assert_eq!((sel.id, sel.status), (s1, SourceStatus::Pending));
        l.set_source_audio(s1, "h1", 2000).unwrap();
        let sel = l.selected_source(t).unwrap().unwrap();
        assert_eq!((sel.audio_hash.as_deref(), sel.status), (Some("h1"), SourceStatus::Ready));
        l.set_source_status(s1, SourceStatus::Failed, Some("The file was moved or deleted.")).unwrap();
        assert_eq!(l.selected_source(t).unwrap().unwrap().error.as_deref(), Some("The file was moved or deleted."));
        l.reset_sources_for_hash("h1").unwrap();
        assert_eq!(l.sources_with_hash("h1").unwrap()[0].status, SourceStatus::Pending);
        l.set_lyric_offset(s1, -300).unwrap();
        assert_eq!(l.selected_source(t).unwrap().unwrap().lyric_offset_ms, -300);
    }

    #[test]
    fn separation_rows_roundtrip() {
        let l = lib();
        let mut row = SeparationRow { audio_hash: "h".into(), model_id: "m".into(), chunk_ms: 10_000, chunks_total: 5, chunks_done: 0, status: SepStatus::Running, size_bytes: 0, last_used_at: None };
        l.upsert_separation(&row).unwrap();
        row.chunks_done = 3;
        l.upsert_separation(&row).unwrap();
        l.touch_separation("h", "m", 42).unwrap();
        let got = l.separation("h", "m").unwrap().unwrap();
        assert_eq!((got.chunks_done, got.last_used_at), (3, Some(42)));
        assert_eq!(l.separations().unwrap().len(), 1);
        l.delete_separation("h", "m").unwrap();
        assert!(l.separation("h", "m").unwrap().is_none());
    }

    #[test]
    fn lyrics_and_settings_roundtrip() {
        let l = lib();
        let t = l.add_track(&local("A", None)).unwrap();
        assert!(l.lyrics(t).unwrap().is_none());
        let lines = vec![Line { start_ms: 0, end_ms: 1000, text: "hi there".into(), words: vec![Word { start_ms: 0, end_ms: 500, text: "hi".into() }] }];
        l.set_lyrics(t, LyricsSource::Lrclib, &lines, 7).unwrap();
        let got = l.lyrics(t).unwrap().unwrap();
        assert_eq!((got.source, got.lines, got.fetched_at), (LyricsSource::Lrclib, lines, 7));
        l.set_lyrics(t, LyricsSource::None, &[], 8).unwrap();
        assert!(l.lyrics(t).unwrap().unwrap().lines.is_empty());
        assert_eq!(l.setting("cache_budget_bytes").unwrap(), None);
        l.set_setting("cache_budget_bytes", "123").unwrap();
        assert_eq!(l.setting("cache_budget_bytes").unwrap().as_deref(), Some("123"));
    }

    #[test]
    fn reopening_a_file_keeps_data_and_does_not_rerun_the_schema() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("kara.db");
        let id = Library::open(&p).unwrap().add_track(&local("A", None)).unwrap();
        assert_eq!(Library::open(&p).unwrap().track(id).unwrap().title, "A");
    }
}
```

- [ ] **Step 4: Run tests to verify they fail**

Run: `cargo test -p kara-core library`
Expected: compile errors (`Library` not found).

- [ ] **Step 5: Implement** — above the tests in `library/mod.rs`:

```rust
//! The library database: tracks, collections, audio sources, separation
//! progress, lyrics and settings. One SQLite file (WAL), search via FTS5.

use crate::lyrics::Line;
use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension, Row};
use std::path::Path;

macro_rules! str_enum {
    ($name:ident { $($var:ident => $s:literal),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
        pub enum $name { $($var),+ }
        impl $name {
            pub fn as_str(self) -> &'static str { match self { $(Self::$var => $s),+ } }
        }
        impl std::str::FromStr for $name {
            type Err = anyhow::Error;
            fn from_str(s: &str) -> Result<Self> {
                match s { $($s => Ok(Self::$var),)+ _ => anyhow::bail!("unknown {}: {}", stringify!($name), s) }
            }
        }
        impl rusqlite::types::ToSql for $name {
            fn to_sql(&self) -> rusqlite::Result<rusqlite::types::ToSqlOutput<'_>> { Ok(self.as_str().into()) }
        }
        impl rusqlite::types::FromSql for $name {
            fn column_result(v: rusqlite::types::ValueRef<'_>) -> rusqlite::types::FromSqlResult<Self> {
                v.as_str()?.parse().map_err(|e: anyhow::Error| rusqlite::types::FromSqlError::Other(e.into()))
            }
        }
    };
}

str_enum!(ProviderId { Local => "local", Spotify => "spotify", Apple => "apple" });
str_enum!(CollectionKind { Playlist => "playlist", Album => "album", Artist => "artist" });
str_enum!(SourceKind { File => "file", Link => "link", Match => "match" });
str_enum!(SourceStatus { Pending => "pending", Fetching => "fetching", Ready => "ready", Failed => "failed" });
str_enum!(SepStatus { Queued => "queued", Running => "running", Ready => "ready", Failed => "failed", Cancelled => "cancelled" });
str_enum!(LyricsSource { Lrclib => "lrclib", Embedded => "embedded", None => "none" });

pub struct NewTrack<'a> {
    pub provider: ProviderId,
    pub provider_ref: Option<&'a str>,
    pub title: &'a str,
    pub artist: Option<&'a str>,
    pub album: Option<&'a str>,
    pub duration_ms: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Track {
    pub id: i64,
    pub provider: ProviderId,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
    pub duration_ms: Option<i64>,
    pub vocal_removal: u8,
    pub key_semitones: i8,
}

#[derive(Clone, Debug, PartialEq)]
pub struct CollectionRow {
    pub id: i64,
    pub provider: ProviderId,
    pub kind: CollectionKind,
    pub name: String,
    pub subtitle: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioSource {
    pub id: i64,
    pub track_id: i64,
    pub kind: SourceKind,
    pub uri: String,
    pub label: Option<String>,
    pub audio_hash: Option<String>,
    pub lyric_offset_ms: i64,
    pub status: SourceStatus,
    pub error: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct SeparationRow {
    pub audio_hash: String,
    pub model_id: String,
    pub chunk_ms: u32,
    pub chunks_total: u32,
    pub chunks_done: u32,
    pub status: SepStatus,
    pub size_bytes: i64,
    pub last_used_at: Option<i64>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct LyricsRow {
    pub source: LyricsSource,
    pub lines: Vec<Line>,
    pub fetched_at: i64,
}

const TRACK_COLS: &str = "t.id, t.provider, t.title, t.artist, t.album, t.duration_ms, t.vocal_removal, t.key_semitones";
const SOURCE_COLS: &str = "id, track_id, kind, uri, label, audio_hash, lyric_offset_ms, status, error";
const SEP_COLS: &str = "audio_hash, model_id, chunk_ms, chunks_total, chunks_done, status, size_bytes, last_used_at";

fn track_row(r: &Row) -> rusqlite::Result<Track> {
    Ok(Track {
        id: r.get(0)?,
        provider: r.get(1)?,
        title: r.get(2)?,
        artist: r.get(3)?,
        album: r.get(4)?,
        duration_ms: r.get(5)?,
        vocal_removal: r.get(6)?,
        key_semitones: r.get(7)?,
    })
}

fn source_row(r: &Row) -> rusqlite::Result<AudioSource> {
    Ok(AudioSource {
        id: r.get(0)?,
        track_id: r.get(1)?,
        kind: r.get(2)?,
        uri: r.get(3)?,
        label: r.get(4)?,
        audio_hash: r.get(5)?,
        lyric_offset_ms: r.get(6)?,
        status: r.get(7)?,
        error: r.get(8)?,
    })
}

fn sep_row(r: &Row) -> rusqlite::Result<SeparationRow> {
    Ok(SeparationRow {
        audio_hash: r.get(0)?,
        model_id: r.get(1)?,
        chunk_ms: r.get(2)?,
        chunks_total: r.get(3)?,
        chunks_done: r.get(4)?,
        status: r.get(5)?,
        size_bytes: r.get(6)?,
        last_used_at: r.get(7)?,
    })
}

pub struct Library {
    conn: Connection,
}

impl Library {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        Self::init(Connection::open(path).with_context(|| format!("open {}", path.display()))?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(conn: Connection) -> Result<Self> {
        // journal_mode returns a row, so it can't go through pragma_update.
        conn.query_row("PRAGMA journal_mode = WAL", [], |_| Ok(()))?;
        conn.pragma_update(None, "foreign_keys", "ON")?;
        let version: i64 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
        if version == 0 {
            conn.execute_batch(include_str!("schema.sql"))?;
            conn.pragma_update(None, "user_version", 1)?;
        }
        Ok(Self { conn })
    }

    // ---- tracks ----

    pub fn add_track(&self, t: &NewTrack) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO track (provider, provider_ref, title, artist, album, duration_ms, added_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            params![t.provider, t.provider_ref, t.title, t.artist, t.album, t.duration_ms, crate::now_ms()],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn track(&self, id: i64) -> Result<Track> {
        let sql = format!("SELECT {TRACK_COLS} FROM track t WHERE t.id = ?1");
        self.conn.query_row(&sql, [id], track_row).with_context(|| format!("track {id}"))
    }

    pub fn update_track_meta(&self, id: i64, title: &str, artist: Option<&str>, album: Option<&str>) -> Result<()> {
        self.conn.execute("UPDATE track SET title = ?2, artist = ?3, album = ?4 WHERE id = ?1", params![id, title, artist, album])?;
        Ok(())
    }

    pub fn set_track_settings(&self, id: i64, vocal_removal: u8, key_semitones: i8) -> Result<()> {
        self.conn.execute("UPDATE track SET vocal_removal = ?2, key_semitones = ?3 WHERE id = ?1", params![id, vocal_removal, key_semitones])?;
        Ok(())
    }

    /// Prefix search over title/artist/album; accent-insensitive; never a syntax error.
    pub fn search(&self, query: &str, limit: u32) -> Result<Vec<Track>> {
        let terms: Vec<String> = query.split_whitespace().map(|w| format!("\"{}\"*", w.replace('"', "\"\""))).collect();
        if terms.is_empty() {
            return Ok(Vec::new());
        }
        let sql = format!(
            "SELECT {TRACK_COLS} FROM track_fts JOIN track t ON t.id = track_fts.rowid WHERE track_fts MATCH ?1 ORDER BY rank LIMIT ?2"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![terms.join(" "), limit], track_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // ---- collections ----

    pub fn upsert_collection(&self, provider: ProviderId, kind: CollectionKind, provider_ref: &str, name: &str, subtitle: Option<&str>) -> Result<i64> {
        Ok(self.conn.query_row(
            "INSERT INTO collection (provider, kind, provider_ref, name, subtitle) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT (provider, kind, provider_ref) DO UPDATE SET name = excluded.name, subtitle = excluded.subtitle
             RETURNING id",
            params![provider, kind, provider_ref, name, subtitle],
            |r| r.get(0),
        )?)
    }

    /// Appends at the end; adding a track that is already there does nothing.
    pub fn add_to_collection(&self, collection_id: i64, track_id: i64) -> Result<()> {
        self.conn.execute(
            "INSERT OR IGNORE INTO collection_track (collection_id, track_id, position)
             VALUES (?1, ?2, (SELECT COALESCE(MAX(position) + 1, 0) FROM collection_track WHERE collection_id = ?1))",
            params![collection_id, track_id],
        )?;
        Ok(())
    }

    /// `provider = None` is the All view.
    pub fn collections(&self, provider: Option<ProviderId>, kind: CollectionKind) -> Result<Vec<CollectionRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, provider, kind, name, subtitle FROM collection
             WHERE kind = ?1 AND (?2 IS NULL OR provider = ?2) ORDER BY name COLLATE NOCASE",
        )?;
        let rows = stmt.query_map(params![kind, provider], |r| {
            Ok(CollectionRow { id: r.get(0)?, provider: r.get(1)?, kind: r.get(2)?, name: r.get(3)?, subtitle: r.get(4)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn collection_tracks(&self, collection_id: i64) -> Result<Vec<Track>> {
        let sql = format!(
            "SELECT {TRACK_COLS} FROM collection_track ct JOIN track t ON t.id = ct.track_id WHERE ct.collection_id = ?1 ORDER BY ct.position"
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([collection_id], track_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    // ---- audio sources ----

    /// The first source added to a track becomes its selected source.
    pub fn add_source(&self, track_id: i64, kind: SourceKind, uri: &str, label: Option<&str>) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO audio_source (track_id, kind, uri, label, status, selected)
             VALUES (?1, ?2, ?3, ?4, 'pending', NOT EXISTS (SELECT 1 FROM audio_source WHERE track_id = ?1))",
            params![track_id, kind, uri, label],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn selected_source(&self, track_id: i64) -> Result<Option<AudioSource>> {
        let sql = format!("SELECT {SOURCE_COLS} FROM audio_source WHERE track_id = ?1 AND selected = 1");
        Ok(self.conn.query_row(&sql, [track_id], source_row).optional()?)
    }

    pub fn sources_with_hash(&self, hash: &str) -> Result<Vec<AudioSource>> {
        let sql = format!("SELECT {SOURCE_COLS} FROM audio_source WHERE audio_hash = ?1");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([hash], source_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn set_source_status(&self, id: i64, status: SourceStatus, error: Option<&str>) -> Result<()> {
        self.conn.execute("UPDATE audio_source SET status = ?2, error = ?3 WHERE id = ?1", params![id, status, error])?;
        Ok(())
    }

    /// Records the standardized audio's hash; the source becomes ready.
    pub fn set_source_audio(&self, id: i64, hash: &str, duration_ms: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE audio_source SET audio_hash = ?2, duration_ms = ?3, status = 'ready', error = NULL WHERE id = ?1",
            params![id, hash, duration_ms],
        )?;
        self.conn.execute(
            "UPDATE track SET duration_ms = COALESCE(duration_ms, ?2) WHERE id = (SELECT track_id FROM audio_source WHERE id = ?1)",
            params![id, duration_ms],
        )?;
        Ok(())
    }

    /// After eviction: these sources must be fetched again before playing.
    pub fn reset_sources_for_hash(&self, hash: &str) -> Result<()> {
        self.conn.execute("UPDATE audio_source SET status = 'pending' WHERE audio_hash = ?1", [hash])?;
        Ok(())
    }

    pub fn set_lyric_offset(&self, source_id: i64, ms: i64) -> Result<()> {
        self.conn.execute("UPDATE audio_source SET lyric_offset_ms = ?2 WHERE id = ?1", params![source_id, ms])?;
        Ok(())
    }

    // ---- separation ----

    pub fn separation(&self, hash: &str, model_id: &str) -> Result<Option<SeparationRow>> {
        let sql = format!("SELECT {SEP_COLS} FROM separation WHERE audio_hash = ?1 AND model_id = ?2");
        Ok(self.conn.query_row(&sql, [hash, model_id], sep_row).optional()?)
    }

    pub fn separations(&self) -> Result<Vec<SeparationRow>> {
        let sql = format!("SELECT {SEP_COLS} FROM separation");
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map([], sep_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn upsert_separation(&self, r: &SeparationRow) -> Result<()> {
        self.conn.execute(
            &format!("INSERT OR REPLACE INTO separation ({SEP_COLS}) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)"),
            params![r.audio_hash, r.model_id, r.chunk_ms, r.chunks_total, r.chunks_done, r.status, r.size_bytes, r.last_used_at],
        )?;
        Ok(())
    }

    pub fn touch_separation(&self, hash: &str, model_id: &str, at_ms: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE separation SET last_used_at = ?3 WHERE audio_hash = ?1 AND model_id = ?2",
            params![hash, model_id, at_ms],
        )?;
        Ok(())
    }

    pub fn delete_separation(&self, hash: &str, model_id: &str) -> Result<()> {
        self.conn.execute("DELETE FROM separation WHERE audio_hash = ?1 AND model_id = ?2", [hash, model_id])?;
        Ok(())
    }

    // ---- lyrics ----

    pub fn lyrics(&self, track_id: i64) -> Result<Option<LyricsRow>> {
        let row = self
            .conn
            .query_row("SELECT source, lines, fetched_at FROM lyrics WHERE track_id = ?1", [track_id], |r| {
                Ok((r.get::<_, LyricsSource>(0)?, r.get::<_, Option<String>>(1)?, r.get::<_, i64>(2)?))
            })
            .optional()?;
        row.map(|(source, lines, fetched_at)| {
            let lines = match lines {
                Some(json) => serde_json::from_str(&json)?,
                None => Vec::new(),
            };
            Ok(LyricsRow { source, lines, fetched_at })
        })
        .transpose()
    }

    pub fn set_lyrics(&self, track_id: i64, source: LyricsSource, lines: &[Line], at_ms: i64) -> Result<()> {
        let json = if lines.is_empty() { None } else { Some(serde_json::to_string(lines)?) };
        self.conn.execute(
            "INSERT OR REPLACE INTO lyrics (track_id, source, lines, fetched_at) VALUES (?1, ?2, ?3, ?4)",
            params![track_id, source, json, at_ms],
        )?;
        Ok(())
    }

    // ---- settings ----

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self.conn.query_row("SELECT value FROM setting WHERE key = ?1", [key], |r| r.get(0)).optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute("INSERT OR REPLACE INTO setting (key, value) VALUES (?1, ?2)", [key, value])?;
        Ok(())
    }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p kara-core library`
Expected: 9 passed.

- [ ] **Step 7: Commit**

```bash
git add crates/kara-core
git commit -m "feat(core): library database with search"
```

---

### Task 7: Lyrics — LRC parsing, word timing, lookup

**Files:**
- Modify: `crates/kara-core/src/lyrics.rs`

**Interfaces:**
- Consumes: `library::LyricsSource`, `lyrics::{Line, Word}` (Task 6).
- Produces:
  - `lyrics::parse_lrc(text: &str, duration_ms: i64) -> Vec<Line>`
  - `lyrics::is_synced(text: &str) -> bool`
  - `lyrics::LyricsFetcher { fn fetch(&self, title: &str, artist: &str, album: Option<&str>, duration_s: u64) -> Result<Option<String>> }` (returns synced LRC text)
  - `lyrics::Lrclib::new() -> Result<Lrclib>` (implements `LyricsFetcher`)
  - `lyrics::find(embedded: Option<&str>, title: &str, artist: Option<&str>, album: Option<&str>, duration_ms: i64, fetcher: &dyn LyricsFetcher) -> Result<(LyricsSource, Vec<Line>)>`

- [ ] **Step 1: Write the failing tests** — append to `lyrics.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn parses_lines_and_times() {
        let lines = parse_lrc("[ar:Someone]\n[00:02.00]Counting windows home\n[00:06.50]Every city light\n", 12_000);
        assert_eq!(lines.len(), 2);
        assert_eq!((lines[0].start_ms, lines[0].end_ms), (2_000, 6_500));
        assert_eq!((lines[1].start_ms, lines[1].end_ms), (6_500, 12_000));
        assert_eq!(lines[1].text, "Every city light");
    }

    #[test]
    fn handles_multiple_stamps_precision_and_blank_breaks() {
        let lines = parse_lrc("[00:01.5][00:20.123]Chorus line\n[00:04.00]\n[00:10.00]Verse\n", 30_000);
        let starts: Vec<_> = lines.iter().map(|l| l.start_ms).collect();
        assert_eq!(starts, vec![1_500, 10_000, 20_123]);
        // The blank line at 4.0 s ends the first line early.
        assert_eq!(lines[0].end_ms, 4_000);
    }

    #[test]
    fn words_are_spread_but_never_slower_than_600ms_each() {
        let lines = parse_lrc("[00:00.00]one two three\n[00:30.00]next\n", 40_000);
        let w = &lines[0].words;
        assert_eq!(w.iter().map(|w| w.text.as_str()).collect::<Vec<_>>(), vec!["one", "two", "three"]);
        assert_eq!((w[0].start_ms, w[1].start_ms, w[2].end_ms), (0, 600, 1_800));
        let quick = parse_lrc("[00:00.00]a b\n[00:01.00]c\n", 2_000);
        assert_eq!(quick[0].words[1].end_ms, 1_000);
    }

    #[test]
    fn last_line_without_duration_gets_five_seconds() {
        let lines = parse_lrc("[01:00.00]end\n", 0);
        assert_eq!(lines[0].end_ms, 65_000);
    }

    #[test]
    fn detects_synced_text() {
        assert!(is_synced("[00:01.00]hi"));
        assert!(!is_synced("just words\nno stamps"));
        assert!(!is_synced("[ar:Artist]\nplain"));
    }

    struct Fake(Option<&'static str>, RefCell<u32>);
    impl LyricsFetcher for Fake {
        fn fetch(&self, _: &str, _: &str, _: Option<&str>, _: u64) -> Result<Option<String>> {
            *self.1.borrow_mut() += 1;
            Ok(self.0.map(String::from))
        }
    }

    #[test]
    fn find_prefers_embedded_then_fetches_then_gives_up() {
        let f = Fake(Some("[00:01.00]online"), RefCell::new(0));
        let (src, lines) = find(Some("[00:00.00]embedded"), "t", Some("a"), None, 5_000, &f).unwrap();
        assert_eq!((src, lines[0].text.as_str(), *f.1.borrow()), (LyricsSource::Embedded, "embedded", 0));
        let (src, lines) = find(Some("plain unsynced"), "t", Some("a"), None, 5_000, &f).unwrap();
        assert_eq!((src, lines[0].text.as_str()), (LyricsSource::Lrclib, "online"));
        let none = Fake(None, RefCell::new(0));
        assert_eq!(find(None, "t", Some("a"), None, 5_000, &none).unwrap(), (LyricsSource::None, vec![]));
        // No artist: don't even ask.
        assert_eq!(find(None, "t", None, None, 5_000, &f).unwrap().0, LyricsSource::None);
    }

    #[test]
    #[ignore = "network"]
    fn lrclib_live_lookup_does_not_error() {
        let l = Lrclib::new().unwrap();
        l.fetch("Bohemian Rhapsody", "Queen", None, 355).unwrap();
    }
}
```

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p kara-core lyrics`
Expected: compile errors (`parse_lrc`, `find` not found).

- [ ] **Step 3: Implement** — add below the `Line`/`Word` types in `lyrics.rs`:

```rust
use crate::library::LyricsSource;
use anyhow::Result;
use std::time::Duration;

/// A word never takes longer than this, so a line before a long instrumental
/// break is sung at a natural pace instead of crawling.
const MAX_WORD_MS: i64 = 600;
const LAST_LINE_MS: i64 = 5_000;

fn parse_timestamp(s: &str) -> Option<i64> {
    let (m, rest) = s.split_once(':')?;
    let m: i64 = m.trim().parse().ok()?;
    let (sec, frac) = rest.split_once('.').unwrap_or((rest, ""));
    let sec: i64 = sec.parse().ok()?;
    if !(0..60).contains(&sec) {
        return None;
    }
    let ms = match frac.len() {
        0 => 0,
        1 => frac.parse::<i64>().ok()? * 100,
        2 => frac.parse::<i64>().ok()? * 10,
        _ => frac.get(..3)?.parse::<i64>().ok()?,
    };
    Some(m * 60_000 + sec * 1_000 + ms)
}

pub fn is_synced(text: &str) -> bool {
    text.lines().any(|l| {
        l.trim_start()
            .strip_prefix('[')
            .and_then(|s| s.split_once(']'))
            .is_some_and(|(t, _)| parse_timestamp(t).is_some())
    })
}

fn spread_words(text: &str, start: i64, end: i64) -> Vec<Word> {
    let words: Vec<&str> = text.split_whitespace().collect();
    let d = ((end - start) / words.len().max(1) as i64).clamp(1, MAX_WORD_MS);
    words
        .iter()
        .enumerate()
        .map(|(i, w)| Word { start_ms: start + i as i64 * d, end_ms: start + (i as i64 + 1) * d, text: w.to_string() })
        .collect()
}

/// LRC text → timed lines with evenly spread word timings. Metadata tags and
/// blank lines produce no line (a blank line still ends the line before it).
pub fn parse_lrc(text: &str, duration_ms: i64) -> Vec<Line> {
    let mut stamped: Vec<(i64, String)> = Vec::new();
    for raw in text.lines() {
        let mut rest = raw.trim();
        let mut times = Vec::new();
        while let Some(inner) = rest.strip_prefix('[') {
            let Some(end) = inner.find(']') else { break };
            match parse_timestamp(&inner[..end]) {
                Some(t) => times.push(t),
                None => break,
            }
            rest = &inner[end + 1..];
        }
        let words = rest.trim().to_string();
        for t in times {
            stamped.push((t, words.clone()));
        }
    }
    stamped.sort_by_key(|(t, _)| *t);
    let mut lines = Vec::new();
    for (i, (start, text)) in stamped.iter().enumerate() {
        if text.is_empty() {
            continue;
        }
        let end = match stamped.get(i + 1) {
            Some((next, _)) => *next,
            None => duration_ms.max(start + LAST_LINE_MS),
        };
        lines.push(Line { start_ms: *start, end_ms: end, text: text.clone(), words: spread_words(text, *start, end) });
    }
    lines
}

pub trait LyricsFetcher {
    /// Synced (LRC) lyrics for a song, or `None` if the service doesn't have them.
    fn fetch(&self, title: &str, artist: &str, album: Option<&str>, duration_s: u64) -> Result<Option<String>>;
}

/// lrclib.net — free, no key.
pub struct Lrclib {
    client: reqwest::blocking::Client,
}

impl Lrclib {
    pub fn new() -> Result<Self> {
        let client = reqwest::blocking::Client::builder()
            .user_agent(concat!("kara-always-oki/", env!("CARGO_PKG_VERSION")))
            .timeout(Duration::from_secs(10))
            .build()?;
        Ok(Self { client })
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct LrclibHit {
    synced_lyrics: Option<String>,
}

impl LyricsFetcher for Lrclib {
    fn fetch(&self, title: &str, artist: &str, album: Option<&str>, duration_s: u64) -> Result<Option<String>> {
        let mut q = vec![("track_name", title.to_string()), ("artist_name", artist.to_string()), ("duration", duration_s.to_string())];
        if let Some(a) = album {
            q.push(("album_name", a.to_string()));
        }
        let resp = self.client.get("https://lrclib.net/api/get").query(&q).send()?;
        if resp.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }
        let hit: LrclibHit = resp.error_for_status()?.json()?;
        Ok(hit.synced_lyrics.filter(|s| is_synced(s)))
    }
}

/// Embedded synced lyrics first, then the online service, else none.
pub fn find(
    embedded: Option<&str>,
    title: &str,
    artist: Option<&str>,
    album: Option<&str>,
    duration_ms: i64,
    fetcher: &dyn LyricsFetcher,
) -> Result<(LyricsSource, Vec<Line>)> {
    if let Some(text) = embedded.filter(|t| is_synced(t)) {
        return Ok((LyricsSource::Embedded, parse_lrc(text, duration_ms)));
    }
    if let Some(artist) = artist {
        if let Some(text) = fetcher.fetch(title, artist, album, (duration_ms / 1000) as u64)? {
            return Ok((LyricsSource::Lrclib, parse_lrc(&text, duration_ms)));
        }
    }
    Ok((LyricsSource::None, Vec::new()))
}
```

Change the file's existing `use serde::{Deserialize, Serialize};` line if needed so `Deserialize` is in scope for `LrclibHit` (it already is).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p kara-core lyrics`
Expected: 6 passed, 1 ignored.

- [ ] **Step 5: Commit**

```bash
git add crates/kara-core/src/lyrics.rs
git commit -m "feat(core): synced lyrics parsing, word timing, LRCLIB lookup"
```

---

### Task 8: Ingest — files, links, yt-dlp

**Files:**
- Create: `crates/kara-core/src/ingest/mod.rs`, `crates/kara-core/src/ingest/link.rs`, `crates/kara-core/src/ingest/ytdlp.rs`
- Modify: `crates/kara-core/src/lib.rs` (add `pub mod ingest;`), `crates/kara-core/Cargo.toml` (add `url = "2"`)

**Interfaces:**
- Consumes: `Library` + enums (Task 6), `audio::read_tags` (Task 1), `Store`, `assets::sha256_hex`, `store::write_atomic` (Task 4).
- Produces:
  - `ingest::link::{LinkVerdict { AudioFile, Extractable, Streaming, Unsupported }, parse_link(&str) -> Option<Url>, verdict(&Url) -> LinkVerdict, rejection_message(&Url) -> Option<String>}`
  - `ingest::ytdlp::{ensure(bin_dir) -> Result<PathBuf>, download(bin, url, out_dir) -> Result<Fetched>, sha_from_sums(sums, name) -> Option<String>, clean_meta(title, uploader: Option<&str>) -> (String, Option<String>)}`; `Fetched { path: PathBuf, title: String, artist: Option<String>, album: Option<String> }`
  - `ingest::{Ingested { track_id, source_id: i64 }, add_file(&Library, &Path) -> Result<Ingested>, add_link(&Library, &Url) -> Result<Ingested>, link_collections(&Library, track_id, artist: Option<&str>, album: Option<&str>) -> Result<()>, fetch_audio(&Library, &Store, &Track, &AudioSource) -> Result<PathBuf>, title_from_file_name(&str) -> String}`

- [ ] **Step 1: Write the failing tests**

`ingest/link.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    fn v(s: &str) -> LinkVerdict {
        verdict(&parse_link(s).unwrap())
    }

    #[test]
    fn parses_links_but_not_search_text() {
        assert!(parse_link("https://youtu.be/abc").is_some());
        assert!(parse_link("soundcloud.com/artist/song").is_some());
        assert!(parse_link("paper satellites").is_none());
        assert!(parse_link("mina").is_none());
    }

    #[test]
    fn classifies_links() {
        assert_eq!(v("https://www.youtube.com/watch?v=x"), LinkVerdict::Extractable);
        assert_eq!(v("https://m.youtube.com/watch?v=x"), LinkVerdict::Extractable);
        assert_eq!(v("https://artist.bandcamp.com/track/x"), LinkVerdict::Extractable);
        assert_eq!(v("https://example.com/files/song.FLAC?dl=1"), LinkVerdict::AudioFile);
        assert_eq!(v("https://open.spotify.com/track/x"), LinkVerdict::Streaming);
        assert_eq!(v("https://music.apple.com/us/album/x"), LinkVerdict::Streaming);
        assert_eq!(v("https://example.com/page"), LinkVerdict::Unsupported);
        assert_eq!(v("https://notyoutube.com/x"), LinkVerdict::Unsupported);
    }

    #[test]
    fn rejection_messages_are_plain() {
        let m = rejection_message(&parse_link("https://open.spotify.com/track/x").unwrap()).unwrap();
        assert!(m.contains("spotify.com links can't be downloaded"));
        let m = rejection_message(&parse_link("https://example.com/page").unwrap()).unwrap();
        assert!(m.contains("Can't get audio from example.com"));
        assert!(rejection_message(&parse_link("https://youtu.be/x").unwrap()).is_none());
    }
}
```

`ingest/ytdlp.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_checksum_in_sums_file() {
        let sums = "aaa  yt-dlp\nbbb  yt-dlp_macos\nccc  yt-dlp_macos.zip\n";
        assert_eq!(sha_from_sums(sums, "yt-dlp_macos").as_deref(), Some("bbb"));
        assert_eq!(sha_from_sums(sums, "nope"), None);
    }

    #[test]
    fn cleans_video_titles() {
        assert_eq!(
            clean_meta("Kiko & the Late Shift - Midnight Laundromat (Official Video)", Some("KikoVEVO")),
            ("Midnight Laundromat".to_string(), Some("Kiko & the Late Shift".to_string()))
        );
        assert_eq!(clean_meta("Overpass Karaoke", Some("Dani Sato - Topic")), ("Overpass Karaoke".to_string(), Some("Dani Sato".to_string())));
        assert_eq!(clean_meta("Song [Lyrics] (HD)", None), ("Song".to_string(), None));
        assert_eq!(clean_meta("Song (feat. Someone)", None).0, "Song (feat. Someone)");
    }
}
```

`ingest/mod.rs`:
```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{CollectionKind, ProviderId, SourceStatus};
    use crate::test_util::write_sine_wav;

    #[test]
    fn file_names_become_titles() {
        assert_eq!(title_from_file_name("salt-and_static.flac"), "salt and static");
        assert_eq!(title_from_file_name("noext"), "noext");
    }

    #[test]
    fn add_file_creates_track_source_and_imported_playlist() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("rooftop-static.wav");
        write_sine_wav(&p, 44_100, 2, 1.0, 440.0);
        let lib = Library::open_in_memory().unwrap();
        let ing = add_file(&lib, &p).unwrap();
        let t = lib.track(ing.track_id).unwrap();
        assert_eq!((t.title.as_str(), t.provider, t.duration_ms), ("rooftop static", ProviderId::Local, Some(1000)));
        let src = lib.selected_source(ing.track_id).unwrap().unwrap();
        assert_eq!((src.kind, src.status, src.uri), (SourceKind::File, SourceStatus::Pending, p.display().to_string()));
        let pls = lib.collections(Some(ProviderId::Local), CollectionKind::Playlist).unwrap();
        assert_eq!(pls[0].name, "Imported");
        assert_eq!(lib.collection_tracks(pls[0].id).unwrap()[0].id, ing.track_id);
    }

    #[test]
    fn link_collections_adds_album_and_artist_once() {
        let lib = Library::open_in_memory().unwrap();
        let a = lib.add_track(&crate::library::NewTrack { provider: ProviderId::Local, provider_ref: None, title: "A", artist: None, album: None, duration_ms: None }).unwrap();
        link_collections(&lib, a, Some("Juniper Row"), Some("Demos 2026")).unwrap();
        link_collections(&lib, a, Some("Juniper Row"), Some("Demos 2026")).unwrap(); // second time is a no-op
        assert_eq!(lib.collections(None, CollectionKind::Artist).unwrap().len(), 1);
        let albums = lib.collections(None, CollectionKind::Album).unwrap();
        assert_eq!((albums.len(), albums[0].subtitle.as_deref()), (1, Some("Juniper Row")));
    }

    #[test]
    fn add_link_refuses_streaming_links() {
        let lib = Library::open_in_memory().unwrap();
        let url = link::parse_link("https://open.spotify.com/track/x").unwrap();
        assert!(add_link(&lib, &url).is_err());
        let url = link::parse_link("https://youtu.be/abc").unwrap();
        let ing = add_link(&lib, &url).unwrap();
        assert_eq!(lib.track(ing.track_id).unwrap().title, "youtu.be link");
    }

    #[test]
    fn fetching_a_moved_file_says_so() {
        let dir = tempfile::tempdir().unwrap();
        let p = dir.path().join("gone.wav");
        write_sine_wav(&p, 44_100, 2, 0.2, 440.0);
        let lib = Library::open_in_memory().unwrap();
        let ing = add_file(&lib, &p).unwrap();
        std::fs::remove_file(&p).unwrap();
        let t = lib.track(ing.track_id).unwrap();
        let s = lib.selected_source(ing.track_id).unwrap().unwrap();
        let err = fetch_audio(&lib, &Store::new(dir.path()), &t, &s).unwrap_err();
        assert_eq!(err.to_string(), "The file was moved or deleted.");
    }
}
```

Add `pub mod ingest;` to `lib.rs` and `url = "2"` to kara-core `[dependencies]`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p kara-core ingest`
Expected: compile errors.

- [ ] **Step 3: Implement `link.rs`** (above its tests):

```rust
//! Which pasted links we can get audio from, and what to tell the user otherwise.

use url::Url;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LinkVerdict {
    /// A direct link to an audio file we can decode.
    AudioFile,
    /// A page yt-dlp can pull audio from.
    Extractable,
    /// A streaming service; its audio can't be downloaded.
    Streaming,
    Unsupported,
}

const EXTRACTABLE: &[&str] = &["youtube.com", "youtu.be", "soundcloud.com", "bandcamp.com", "vimeo.com", "archive.org", "mixcloud.com"];
const STREAMING: &[&str] = &["spotify.com", "music.apple.com", "tidal.com", "deezer.com"];
const AUDIO_EXT: &[&str] = &["mp3", "wav", "flac", "m4a", "aac", "ogg", "aif", "aiff"];

fn host_is(host: &str, domain: &str) -> bool {
    host == domain || host.ends_with(&format!(".{domain}"))
}

/// Text that looks like a link (with or without https://). Search text returns None.
pub fn parse_link(input: &str) -> Option<Url> {
    let s = input.trim();
    if s.is_empty() || s.chars().any(char::is_whitespace) {
        return None;
    }
    let url = if s.starts_with("http://") || s.starts_with("https://") {
        Url::parse(s).ok()?
    } else {
        Url::parse(&format!("https://{s}")).ok()?
    };
    url.host_str().filter(|h| h.contains('.'))?;
    Some(url)
}

pub fn verdict(url: &Url) -> LinkVerdict {
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();
    let ext = url.path().rsplit('.').next().unwrap_or("").to_ascii_lowercase();
    if url.path().contains('.') && AUDIO_EXT.contains(&ext.as_str()) {
        LinkVerdict::AudioFile
    } else if EXTRACTABLE.iter().any(|d| host_is(&host, d)) {
        LinkVerdict::Extractable
    } else if STREAMING.iter().any(|d| host_is(&host, d)) {
        LinkVerdict::Streaming
    } else {
        LinkVerdict::Unsupported
    }
}

/// What to show under the search bar for a link we won't process.
pub fn rejection_message(url: &Url) -> Option<String> {
    let host = url.host_str().unwrap_or("").trim_start_matches("www.").trim_start_matches("open.");
    match verdict(url) {
        LinkVerdict::AudioFile | LinkVerdict::Extractable => None,
        LinkVerdict::Streaming => Some(format!("{host} links can't be downloaded. Search for the song instead.")),
        LinkVerdict::Unsupported => Some(format!(
            "Can't get audio from {host}. Paste a YouTube, SoundCloud or Bandcamp link, or a direct link to an audio file."
        )),
    }
}
```

- [ ] **Step 4: Implement `ytdlp.rs`** (above its tests):

```rust
//! yt-dlp: installed on first use (checksum-verified), then used to pull audio from pages.

use crate::assets::sha256_hex;
use crate::store::write_atomic;
use anyhow::{bail, Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};
use std::process::Command;

const BIN_NAME: &str = "yt-dlp_macos";
const BIN_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/yt-dlp_macos";
const SUMS_URL: &str = "https://github.com/yt-dlp/yt-dlp/releases/latest/download/SHA2-256SUMS";

pub fn sha_from_sums(sums: &str, name: &str) -> Option<String> {
    sums.lines().find_map(|l| {
        let (hash, file) = l.split_once(char::is_whitespace)?;
        (file.trim() == name).then(|| hash.to_string())
    })
}

/// Path to a verified yt-dlp in `bin_dir`, installing it on first use.
pub fn ensure(bin_dir: &Path) -> Result<PathBuf> {
    let path = bin_dir.join("yt-dlp");
    if path.exists() {
        return Ok(path);
    }
    let get = |url: &str| -> Result<Vec<u8>> {
        Ok(reqwest::blocking::get(url)?.error_for_status()?.bytes()?.to_vec())
    };
    let sums = String::from_utf8(get(SUMS_URL)?)?;
    let want = sha_from_sums(&sums, BIN_NAME).context("yt-dlp checksum not published")?;
    let bytes = get(BIN_URL)?;
    if sha256_hex(&bytes) != want {
        bail!("yt-dlp failed its checksum");
    }
    write_atomic(&path, &bytes)?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    Ok(path)
}

const NOISE: &[&str] = &["official", "lyric", "lyrics", "audio", "video", "mv", "hd", "4k", "visualizer"];

fn strip_noise(s: &str) -> String {
    let mut out = s.trim().to_string();
    loop {
        let t = out.trim_end();
        let Some(close) = t.chars().last().filter(|c| *c == ')' || *c == ']') else { break };
        let open = if close == ')' { '(' } else { '[' };
        let Some(i) = t.rfind(open) else { break };
        let inner = t[i + 1..t.len() - 1].to_lowercase();
        if inner.split(|c: char| !c.is_alphanumeric()).any(|w| NOISE.contains(&w)) {
            out = t[..i].trim_end().to_string();
        } else {
            break;
        }
    }
    out
}

/// Video title + channel → (song title, artist). Handles "Artist - Title (Official Video)"
/// and YouTube Music's "Artist - Topic" channels.
pub fn clean_meta(title: &str, uploader: Option<&str>) -> (String, Option<String>) {
    let title = strip_noise(title);
    if let Some((artist, song)) = title.split_once(" - ") {
        return (strip_noise(song), Some(artist.trim().to_string()));
    }
    (title, uploader.map(|u| u.trim_end_matches(" - Topic").trim().to_string()))
}

pub struct Fetched {
    pub path: PathBuf,
    pub title: String,
    pub artist: Option<String>,
    pub album: Option<String>,
}

#[derive(Deserialize)]
struct Info {
    filepath: PathBuf,
    title: Option<String>,
    uploader: Option<String>,
    artist: Option<String>,
    track: Option<String>,
    album: Option<String>,
}

/// Downloads the best audio we can decode (M4A, else MP3, else whatever is best).
pub fn download(bin: &Path, url: &str, out_dir: &Path) -> Result<Fetched> {
    std::fs::create_dir_all(out_dir)?;
    let out = Command::new(bin)
        .args(["--no-playlist", "--no-progress", "-f", "bestaudio[ext=m4a]/bestaudio[ext=mp3]/bestaudio"])
        .arg("-o")
        .arg(out_dir.join("%(id)s.%(ext)s"))
        .args(["--print", "after_move:%(.{filepath,title,uploader,artist,track,album})j"])
        .arg(url)
        .output()
        .context("run yt-dlp")?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        bail!("{}", err.lines().last().unwrap_or("yt-dlp failed"));
    }
    let line = String::from_utf8_lossy(&out.stdout).lines().last().unwrap_or("").to_string();
    let info: Info = serde_json::from_str(&line).context("read yt-dlp output")?;
    let (title, artist) = match info.track {
        Some(track) => (track, info.artist.or(info.uploader)),
        None => clean_meta(info.title.as_deref().unwrap_or("Unknown song"), info.uploader.as_deref()),
    };
    Ok(Fetched { path: info.filepath, title, artist, album: info.album })
}
```

- [ ] **Step 5: Implement `ingest/mod.rs`** (above its tests):

```rust
//! Adding songs (one file or one link) and getting their audio onto disk.

pub mod link;
pub mod ytdlp;

use crate::audio;
use crate::library::{AudioSource, CollectionKind, Library, NewTrack, ProviderId, SourceKind, Track};
use crate::store::Store;
use anyhow::{anyhow, bail, Context, Result};
use link::LinkVerdict;
use std::path::{Path, PathBuf};
use url::Url;

pub struct Ingested {
    pub track_id: i64,
    pub source_id: i64,
}

/// "salt-and_static.flac" → "salt and static"
pub fn title_from_file_name(name: &str) -> String {
    let stem = Path::new(name).file_stem().and_then(|s| s.to_str()).unwrap_or(name);
    stem.split(|c| c == '-' || c == '_').filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" ").trim().to_string()
}

/// Puts a local track in Local › Imported, plus its album and artist collections.
pub fn link_collections(lib: &Library, track_id: i64, artist: Option<&str>, album: Option<&str>) -> Result<()> {
    let imported = lib.upsert_collection(ProviderId::Local, CollectionKind::Playlist, "imported", "Imported", None)?;
    lib.add_to_collection(imported, track_id)?;
    if let Some(artist) = artist {
        let key = format!("artist:{}", artist.to_lowercase());
        let id = lib.upsert_collection(ProviderId::Local, CollectionKind::Artist, &key, artist, None)?;
        lib.add_to_collection(id, track_id)?;
    }
    if let Some(album) = album {
        let key = format!("album:{}:{}", artist.unwrap_or("").to_lowercase(), album.to_lowercase());
        let id = lib.upsert_collection(ProviderId::Local, CollectionKind::Album, &key, album, artist)?;
        lib.add_to_collection(id, track_id)?;
    }
    Ok(())
}

pub fn add_file(lib: &Library, path: &Path) -> Result<Ingested> {
    let (tags, duration_ms) = audio::read_tags(path).context("This file isn't audio we can play.")?;
    let name = path.file_name().and_then(|n| n.to_str()).unwrap_or("Untitled");
    let title = tags.title.clone().unwrap_or_else(|| title_from_file_name(name));
    let track_id = lib.add_track(&NewTrack {
        provider: ProviderId::Local,
        provider_ref: None,
        title: &title,
        artist: tags.artist.as_deref(),
        album: tags.album.as_deref(),
        duration_ms,
    })?;
    let source_id = lib.add_source(track_id, SourceKind::File, &path.display().to_string(), Some(name))?;
    link_collections(lib, track_id, tags.artist.as_deref(), tags.album.as_deref())?;
    Ok(Ingested { track_id, source_id })
}

/// Adds a link as a track right away; its real title arrives when the audio is fetched.
pub fn add_link(lib: &Library, url: &Url) -> Result<Ingested> {
    if let Some(msg) = link::rejection_message(url) {
        bail!(msg);
    }
    let host = url.host_str().unwrap_or("link").trim_start_matches("www.");
    let title = match link::verdict(url) {
        LinkVerdict::AudioFile => title_from_file_name(url.path_segments().and_then(|mut s| s.next_back()).unwrap_or(host)),
        _ => format!("{host} link"),
    };
    let track_id = lib.add_track(&NewTrack { provider: ProviderId::Local, provider_ref: None, title: &title, artist: None, album: None, duration_ms: None })?;
    let source_id = lib.add_source(track_id, SourceKind::Link, url.as_str(), Some(host))?;
    link_collections(lib, track_id, None, None)?;
    Ok(Ingested { track_id, source_id })
}

fn download_file(url: &Url, dir: &Path) -> Result<PathBuf> {
    std::fs::create_dir_all(dir)?;
    let name = url.path_segments().and_then(|mut s| s.next_back()).filter(|s| !s.is_empty()).unwrap_or("download");
    let path = dir.join(name);
    let mut resp = reqwest::blocking::get(url.as_str())?.error_for_status()?;
    let mut f = std::fs::File::create(&path)?;
    resp.copy_to(&mut f)?;
    Ok(path)
}

/// Gets the source's original audio onto disk and returns its path. Links are
/// downloaded into `store.tmp_dir()` (the caller deletes them after decoding).
/// Error messages are shown to the user as-is.
pub fn fetch_audio(lib: &Library, store: &Store, track: &Track, source: &AudioSource) -> Result<PathBuf> {
    match source.kind {
        SourceKind::File => {
            let p = PathBuf::from(&source.uri);
            if !p.exists() {
                bail!("The file was moved or deleted.");
            }
            Ok(p)
        }
        SourceKind::Link => {
            let url = Url::parse(&source.uri)?;
            match link::verdict(&url) {
                LinkVerdict::AudioFile => download_file(&url, &store.tmp_dir()).context("Couldn't download this song. Check the link and your connection."),
                LinkVerdict::Extractable => {
                    let bin = ytdlp::ensure(&store.bin_dir()).context("Couldn't set up downloading. Check your connection.")?;
                    let f = ytdlp::download(&bin, url.as_str(), &store.tmp_dir())
                        .map_err(|e| anyhow!("Couldn't download this song. ({e})"))?;
                    lib.update_track_meta(track.id, &f.title, f.artist.as_deref(), f.album.as_deref())?;
                    link_collections(lib, track.id, f.artist.as_deref(), f.album.as_deref())?;
                    Ok(f.path)
                }
                _ => bail!(link::rejection_message(&url).unwrap_or_default()),
            }
        }
        SourceKind::Match => bail!("Songs from streaming libraries arrive in a later version."),
    }
}
```

- [ ] **Step 6: Run tests to verify they pass**

Run: `cargo test -p kara-core ingest`
Expected: 10 passed.

- [ ] **Step 7: Commit**

```bash
git add crates/kara-core
git commit -m "feat(core): ingest files and links, yt-dlp install and download"
```

---

### Task 9: Cache — chunk files, finishing, eviction, startup cleanup

**Files:**
- Create: `crates/kara-core/src/cache.rs`
- Modify: `crates/kara-core/src/lib.rs` (add `pub mod cache;`)

**Interfaces:**
- Consumes: `Store`, `Stem`, `write_atomic` (Task 4); `audio::{encode_flac, read_flac, Stereo}`; `mdx::ChunkOut` (Task 3); `Library`, `SeparationRow`, `SepStatus`, `SourceKind` (Task 6).
- Produces:
  - `cache::DEFAULT_BUDGET: u64 = 5 * 1024 * 1024 * 1024`
  - `cache::Entry { size_bytes: i64, last_used_at: i64, protected: bool }`, `pick_evictions(&[Entry], budget: u64) -> Vec<usize>`
  - `write_chunk(&Store, hash, model_id, &ChunkOut) -> Result<()>`, `read_chunk(&Store, hash, model_id, index: u32) -> Result<(Stereo, Stereo)>`, `count_complete_chunks(&Store, hash, model_id) -> u32`
  - `finish(&Store, &Library, hash, model_id) -> Result<()>` (mark ready, record size, delete `source.flac`)
  - `rebuild_mix(&Store, hash, model_id, chunks: u32) -> Result<Stereo>`
  - `budget(&Library) -> Result<u64>`, `enforce_budget(&Store, &Library, protected: &[String]) -> Result<Vec<String>>` (returns evicted hashes)
  - `startup_cleanup(&Store, &Library) -> Result<()>`

- [ ] **Step 1: Write the failing tests** — `crates/kara-core/src/cache.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::{NewTrack, ProviderId};
    use crate::separate::mdx::ChunkOut;

    fn e(size: i64, used: i64, protected: bool) -> Entry {
        Entry { size_bytes: size, last_used_at: used, protected }
    }

    fn chunk(index: usize, len: usize) -> ChunkOut {
        let v: Vec<f32> = (0..len).map(|i| (i as f32 * 0.01).sin() * 0.3).collect();
        let i: Vec<f32> = v.iter().map(|x| x * 0.5).collect();
        ChunkOut { index, vocals: Stereo { left: v.clone(), right: v }, inst: Stereo { left: i.clone(), right: i } }
    }

    fn sep_row(hash: &str, total: u32, status: SepStatus, size: i64, used: i64) -> SeparationRow {
        SeparationRow { audio_hash: hash.into(), model_id: "m".into(), chunk_ms: 10_000, chunks_total: total, chunks_done: total, status, size_bytes: size, last_used_at: Some(used) }
    }

    #[test]
    fn evicts_least_recently_used_until_under_budget() {
        let entries = [e(40, 3, false), e(40, 1, false), e(40, 2, false)];
        assert_eq!(pick_evictions(&entries, 100), vec![1]);
        assert_eq!(pick_evictions(&entries, 40), vec![1, 2]);
        assert!(pick_evictions(&entries, 120).is_empty());
    }

    #[test]
    fn never_evicts_protected_entries() {
        let entries = [e(100, 1, true), e(10, 2, false)];
        assert_eq!(pick_evictions(&entries, 0), vec![1]);
    }

    #[test]
    fn chunks_roundtrip_and_count_only_complete_pairs() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::new(dir.path());
        write_chunk(&s, "h", "m", &chunk(0, 1000)).unwrap();
        write_chunk(&s, "h", "m", &chunk(1, 1000)).unwrap();
        // A lone vocals file for chunk 2 (crash between the two writes) doesn't count.
        std::fs::write(s.chunk_path("h", "m", 2, Stem::Vocals), b"x").unwrap();
        assert_eq!(count_complete_chunks(&s, "h", "m"), 2);
        let (v, i) = read_chunk(&s, "h", "m", 1).unwrap();
        assert_eq!((v.len(), i.len()), (1000, 1000));
    }

    #[test]
    fn rebuilt_mix_is_vocals_plus_instrumental() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::new(dir.path());
        let c = chunk(0, 500);
        write_chunk(&s, "h", "m", &c).unwrap();
        let mix = rebuild_mix(&s, "h", "m", 1).unwrap();
        for k in 0..500 {
            assert!((mix.left[k] - (c.vocals.left[k] + c.inst.left[k])).abs() < 2.0 / 32767.0);
        }
    }

    #[test]
    fn finish_marks_ready_records_size_and_deletes_source() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::new(dir.path());
        let lib = Library::open_in_memory().unwrap();
        write_atomic(&s.source_path("h"), b"src").unwrap();
        write_chunk(&s, "h", "m", &chunk(0, 1000)).unwrap();
        lib.upsert_separation(&sep_row("h", 1, SepStatus::Running, 0, 0)).unwrap();
        finish(&s, &lib, "h", "m").unwrap();
        let row = lib.separation("h", "m").unwrap().unwrap();
        assert_eq!(row.status, SepStatus::Ready);
        assert!(row.size_bytes > 0);
        assert!(!s.source_path("h").exists());
    }

    #[test]
    fn enforce_budget_evicts_and_resets_sources_but_keeps_unrecoverable_files() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::new(dir.path());
        let lib = Library::open_in_memory().unwrap();
        lib.set_setting("cache_budget_bytes", "100").unwrap();
        let t = |name| lib.add_track(&NewTrack { provider: ProviderId::Local, provider_ref: None, title: name, artist: None, album: None, duration_ms: None }).unwrap();
        // "old": a link (can be downloaded again). "gone": a file whose original was deleted.
        let (old, gone) = (t("old"), t("gone"));
        let s_old = lib.add_source(old, SourceKind::Link, "https://youtu.be/x", None).unwrap();
        let s_gone = lib.add_source(gone, SourceKind::File, "/nowhere/gone.wav", None).unwrap();
        lib.set_source_audio(s_old, "old", 1000).unwrap();
        lib.set_source_audio(s_gone, "gone", 1000).unwrap();
        for h in ["old", "gone", "new"] {
            write_chunk(&s, h, "m", &chunk(0, 10)).unwrap();
        }
        lib.upsert_separation(&sep_row("gone", 1, SepStatus::Ready, 80, 1)).unwrap();
        lib.upsert_separation(&sep_row("old", 1, SepStatus::Ready, 80, 2)).unwrap();
        lib.upsert_separation(&sep_row("new", 1, SepStatus::Ready, 80, 3)).unwrap();
        let evicted = enforce_budget(&s, &lib, &["new".to_string()]).unwrap();
        assert_eq!(evicted, vec!["old".to_string()]);
        assert!(!s.stems_dir("old", "m").exists());
        assert!(lib.separation("old", "m").unwrap().is_none());
        assert_eq!(lib.selected_source(old).unwrap().unwrap().status, crate::library::SourceStatus::Pending);
        assert!(s.stems_dir("gone", "m").exists());
    }

    #[test]
    fn startup_cleanup_removes_parts_and_recounts() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::new(dir.path());
        let lib = Library::open_in_memory().unwrap();
        write_chunk(&s, "h", "m", &chunk(0, 100)).unwrap();
        write_chunk(&s, "h", "m", &chunk(1, 100)).unwrap();
        std::fs::write(s.stems_dir("h", "m").join("0002.vocals.flac.part"), b"x").unwrap();
        std::fs::create_dir_all(s.tmp_dir()).unwrap();
        std::fs::write(s.tmp_dir().join("half.m4a"), b"x").unwrap();
        let mut row = sep_row("h", 4, SepStatus::Running, 0, 0);
        row.chunks_done = 3; // DB claimed 3, but only 2 pairs made it to disk
        lib.upsert_separation(&row).unwrap();
        // A song whose chunks all landed but which crashed before `finish`.
        write_chunk(&s, "done", "m", &chunk(0, 100)).unwrap();
        write_atomic(&s.source_path("done"), b"src").unwrap();
        lib.upsert_separation(&sep_row("done", 1, SepStatus::Running, 0, 0)).unwrap();

        startup_cleanup(&s, &lib).unwrap();

        assert!(!s.stems_dir("h", "m").join("0002.vocals.flac.part").exists());
        assert_eq!(std::fs::read_dir(s.tmp_dir()).unwrap().count(), 0);
        let row = lib.separation("h", "m").unwrap().unwrap();
        assert_eq!((row.chunks_done, row.status), (2, SepStatus::Queued));
        assert_eq!(lib.separation("done", "m").unwrap().unwrap().status, SepStatus::Ready);
        assert!(!s.source_path("done").exists());
    }
}
```

Add `pub mod cache;` to `lib.rs`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p kara-core cache`
Expected: compile errors.

- [ ] **Step 3: Implement** — above the tests in `cache.rs`:

```rust
//! The separated-audio cache: chunk files on disk, marking songs ready, the
//! disk budget (least recently played goes first) and cleanup after a crash.

use crate::audio::{encode_flac, read_flac, Stereo};
use crate::library::{Library, SepStatus, SeparationRow, SourceKind};
use crate::separate::mdx::ChunkOut;
use crate::store::{write_atomic, Stem, Store};
use anyhow::{Context, Result};
use std::path::Path;

pub const DEFAULT_BUDGET: u64 = 5 * 1024 * 1024 * 1024;

pub struct Entry {
    pub size_bytes: i64,
    pub last_used_at: i64,
    pub protected: bool,
}

/// Indices to evict, least recently used first, until the total fits `budget`.
pub fn pick_evictions(entries: &[Entry], budget: u64) -> Vec<usize> {
    let mut total: i64 = entries.iter().map(|e| e.size_bytes).sum();
    let mut order: Vec<usize> = (0..entries.len()).filter(|&i| !entries[i].protected).collect();
    order.sort_by_key(|&i| entries[i].last_used_at);
    let mut out = Vec::new();
    for i in order {
        if total <= budget as i64 {
            break;
        }
        total -= entries[i].size_bytes;
        out.push(i);
    }
    out
}

/// Writes the vocals file, then the instrumental file. A chunk counts as done
/// only when both exist.
pub fn write_chunk(store: &Store, hash: &str, model_id: &str, c: &ChunkOut) -> Result<()> {
    let i = c.index as u32;
    write_atomic(&store.chunk_path(hash, model_id, i, Stem::Vocals), &encode_flac(&c.vocals)?)?;
    write_atomic(&store.chunk_path(hash, model_id, i, Stem::Inst), &encode_flac(&c.inst)?)?;
    Ok(())
}

pub fn read_chunk(store: &Store, hash: &str, model_id: &str, index: u32) -> Result<(Stereo, Stereo)> {
    Ok((
        read_flac(&store.chunk_path(hash, model_id, index, Stem::Vocals))?,
        read_flac(&store.chunk_path(hash, model_id, index, Stem::Inst))?,
    ))
}

/// Complete chunk pairs on disk, counting up from 0 and stopping at the first gap.
pub fn count_complete_chunks(store: &Store, hash: &str, model_id: &str) -> u32 {
    let mut n = 0;
    while store.chunk_path(hash, model_id, n, Stem::Vocals).exists() && store.chunk_path(hash, model_id, n, Stem::Inst).exists() {
        n += 1;
    }
    n
}

fn dir_size(dir: &Path) -> i64 {
    std::fs::read_dir(dir)
        .map(|rd| rd.filter_map(|e| e.ok()?.metadata().ok()).map(|m| m.len() as i64).sum())
        .unwrap_or(0)
}

/// All chunks are on disk: mark ready, record the size, drop the now-redundant source.
pub fn finish(store: &Store, lib: &Library, hash: &str, model_id: &str) -> Result<()> {
    let mut row = lib.separation(hash, model_id)?.context("missing separation row")?;
    row.status = SepStatus::Ready;
    row.chunks_done = row.chunks_total;
    row.size_bytes = dir_size(&store.stems_dir(hash, model_id));
    row.last_used_at = Some(crate::now_ms());
    lib.upsert_separation(&row)?;
    let _ = std::fs::remove_file(store.source_path(hash));
    Ok(())
}

/// The original song, rebuilt from its two separated tracks (for re-separating with another model).
pub fn rebuild_mix(store: &Store, hash: &str, model_id: &str, chunks: u32) -> Result<Stereo> {
    let mut mix = Stereo::default();
    for i in 0..chunks {
        let (v, inst) = read_chunk(store, hash, model_id, i)?;
        mix.left.extend(v.left.iter().zip(&inst.left).map(|(a, b)| a + b));
        mix.right.extend(v.right.iter().zip(&inst.right).map(|(a, b)| a + b));
    }
    Ok(mix)
}

pub fn budget(lib: &Library) -> Result<u64> {
    Ok(lib.setting("cache_budget_bytes")?.and_then(|v| v.parse().ok()).unwrap_or(DEFAULT_BUDGET))
}

/// Deletes the separated audio of least recently played songs until the cache
/// fits its budget. Never touches `protected` hashes (playing / queued) or songs
/// from a local file that no longer exists (they couldn't be recreated).
pub fn enforce_budget(store: &Store, lib: &Library, protected: &[String]) -> Result<Vec<String>> {
    let rows: Vec<SeparationRow> = lib.separations()?.into_iter().filter(|r| r.status == SepStatus::Ready).collect();
    let mut entries = Vec::with_capacity(rows.len());
    for r in &rows {
        let unrecoverable = lib
            .sources_with_hash(&r.audio_hash)?
            .iter()
            .any(|s| s.kind == SourceKind::File && !Path::new(&s.uri).exists());
        entries.push(Entry {
            size_bytes: r.size_bytes,
            last_used_at: r.last_used_at.unwrap_or(0),
            protected: unrecoverable || protected.contains(&r.audio_hash),
        });
    }
    let mut evicted = Vec::new();
    for i in pick_evictions(&entries, budget(lib)?) {
        let r = &rows[i];
        match std::fs::remove_dir_all(store.stems_dir(&r.audio_hash, &r.model_id)) {
            Err(e) if e.kind() != std::io::ErrorKind::NotFound => return Err(e.into()),
            _ => {}
        }
        let _ = std::fs::remove_dir(store.audio_dir(&r.audio_hash)); // only if now empty
        lib.delete_separation(&r.audio_hash, &r.model_id)?;
        lib.reset_sources_for_hash(&r.audio_hash)?;
        evicted.push(r.audio_hash.clone());
    }
    Ok(evicted)
}

fn remove_part_files(dir: &Path) -> Result<()> {
    let Ok(rd) = std::fs::read_dir(dir) else { return Ok(()) };
    for entry in rd {
        let p = entry?.path();
        if p.is_dir() {
            remove_part_files(&p)?;
        } else if p.extension().is_some_and(|x| x == "part") {
            std::fs::remove_file(&p)?;
        }
    }
    Ok(())
}

/// Run once at launch: clear temp downloads and half-written files, then make
/// the database agree with the chunk files actually on disk.
pub fn startup_cleanup(store: &Store, lib: &Library) -> Result<()> {
    let _ = std::fs::remove_dir_all(store.tmp_dir());
    std::fs::create_dir_all(store.tmp_dir())?;
    remove_part_files(&store.audio_root())?;
    for mut row in lib.separations()? {
        let done = count_complete_chunks(store, &row.audio_hash, &row.model_id);
        if done >= row.chunks_total {
            if row.status != SepStatus::Ready {
                finish(store, lib, &row.audio_hash, &row.model_id)?;
            }
            continue;
        }
        let status = match row.status {
            SepStatus::Ready | SepStatus::Running => SepStatus::Queued,
            s => s,
        };
        if done != row.chunks_done || status != row.status {
            row.chunks_done = done;
            row.status = status;
            lib.upsert_separation(&row)?;
        }
    }
    Ok(())
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p kara-core cache`
Expected: 7 passed.

- [ ] **Step 5: Commit**

```bash
git add crates/kara-core
git commit -m "feat(core): stem cache with budget eviction and crash cleanup"
```

---

### Task 10: Jobs — the prepare pipeline and the worker queue

**Files:**
- Create: `crates/kara-core/src/jobs.rs`
- Modify: `crates/kara-core/src/lib.rs` (add `pub mod jobs;`)

**Interfaces:**
- Consumes: everything above — `ingest::fetch_audio`, `audio::{decode_file, read_flac, encode_flac, audio_hash, SAMPLE_RATE}`, `lyrics::{find, is_synced, parse_lrc, LyricsFetcher}`, `mdx::{separate, chunk_count, MdxParams, Outcome, VocalModel}`, `cache::{write_chunk, count_complete_chunks, finish, enforce_budget}`, `Library`, `Store`.
- Produces:
  - `jobs::Stage { Fetching, Standardizing, Lyrics, Separating }`
  - `jobs::Event { Stage { track_id, stage }, Progress { track_id, chunks_done: u32, chunks_total: u32 }, Ready { track_id }, Failed { track_id, message: String } }` (derive `Clone, Debug, PartialEq`)
  - `jobs::Ctx { store: Store, model_id: String, params: MdxParams, chunk_len: usize }`
  - `jobs::prepare(ctx: &Ctx, lib: &Library, model: &mut dyn VocalModel, fetcher: &(dyn LyricsFetcher + Sync), track_id: i64, cancel: &AtomicBool, emit: &mut dyn FnMut(Event)) -> Result<Outcome>`
  - `jobs::Worker::spawn(ctx: Ctx, model: Box<dyn VocalModel>, fetcher: Box<dyn LyricsFetcher + Send + Sync>, events: mpsc::Sender<Event>) -> Worker`, `Worker::play(&self, tracks: Vec<i64>)`; dropping a `Worker` stops it.

Pipeline for one song: already ready? → touch and report ready. Else get the standard audio (existing `source.flac`, or fetch → decode → hash → write `source.flac`), reuse any finished separation of the same audio, then separate from the first missing chunk while lyrics are looked up on a second thread. Any failure marks the source failed with a user-facing message and emits `Failed`.

- [ ] **Step 1: Write the failing tests** — `crates/kara-core/src/jobs.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ingest;
    use crate::library::SourceStatus;
    use crate::test_util::{test_params, write_sine_wav, Silence};
    use std::path::Path;
    use std::sync::atomic::Ordering;
    use std::time::Duration;

    struct NoLyrics;
    impl LyricsFetcher for NoLyrics {
        fn fetch(&self, _: &str, _: &str, _: Option<&str>, _: u64) -> Result<Option<String>> {
            Ok(None)
        }
    }

    fn ctx(root: &Path) -> Ctx {
        Ctx { store: Store::new(root), model_id: "test".into(), params: test_params(), chunk_len: 44_100 }
    }

    /// A 2.5 s song = 3 chunks of 1 s.
    fn song(dir: &Path, name: &str) -> std::path::PathBuf {
        let p = dir.join(name);
        write_sine_wav(&p, 44_100, 2, 2.5, 330.0);
        p
    }

    fn run(c: &Ctx, lib: &Library, track: i64, cancel: &AtomicBool) -> (Result<Outcome>, Vec<Event>) {
        let mut events = Vec::new();
        let r = prepare(c, lib, &mut Silence, &NoLyrics, track, cancel, &mut |e| events.push(e));
        (r, events)
    }

    #[test]
    fn prepares_a_local_file_end_to_end() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let t = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        let (r, events) = run(&c, &lib, t, &AtomicBool::new(false));
        assert_eq!(r.unwrap(), Outcome::Done);
        assert_eq!(events.first(), Some(&Event::Stage { track_id: t, stage: Stage::Fetching }));
        assert!(events.contains(&Event::Progress { track_id: t, chunks_done: 3, chunks_total: 3 }));
        assert_eq!(events.last(), Some(&Event::Ready { track_id: t }));
        let hash = lib.selected_source(t).unwrap().unwrap().audio_hash.unwrap();
        assert_eq!(lib.separation(&hash, "test").unwrap().unwrap().status, SepStatus::Ready);
        assert!(!c.store.source_path(&hash).exists());
        assert_eq!(lib.lyrics(t).unwrap().unwrap().source, LyricsSource::None);
        // Second time: instant.
        let (_, again) = run(&c, &lib, t, &AtomicBool::new(false));
        assert_eq!(again, vec![Event::Ready { track_id: t }]);
    }

    #[test]
    fn prepare_resumes_after_cancel() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let t = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        let cancel = AtomicBool::new(false);
        let r = prepare(&c, &lib, &mut Silence, &NoLyrics, t, &cancel, &mut |e| {
            if let Event::Progress { chunks_done: 1, .. } = e {
                cancel.store(true, Ordering::Relaxed);
            }
        });
        assert_eq!(r.unwrap(), Outcome::Cancelled);
        let (r, events) = run(&c, &lib, t, &AtomicBool::new(false));
        assert_eq!(r.unwrap(), Outcome::Done);
        let first_progress = events.iter().find_map(|e| match e {
            Event::Progress { chunks_done, .. } => Some(*chunks_done),
            _ => None,
        });
        assert_eq!(first_progress, Some(1)); // picked up after the one finished chunk
        assert!(!events.iter().any(|e| matches!(e, Event::Stage { stage: Stage::Fetching, .. })));
    }

    #[test]
    fn same_audio_is_separated_once() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let p = song(dir.path(), "a.wav");
        let t1 = ingest::add_file(&lib, &p).unwrap().track_id;
        let t2 = ingest::add_file(&lib, &p).unwrap().track_id;
        run(&c, &lib, t1, &AtomicBool::new(false)).0.unwrap();
        let (_, events) = run(&c, &lib, t2, &AtomicBool::new(false));
        assert!(!events.iter().any(|e| matches!(e, Event::Progress { .. })));
        assert_eq!(events.last(), Some(&Event::Ready { track_id: t2 }));
        let hash = lib.selected_source(t2).unwrap().unwrap().audio_hash.unwrap();
        assert!(!c.store.source_path(&hash).exists());
    }

    #[test]
    fn a_missing_file_fails_with_a_plain_message() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let p = song(dir.path(), "a.wav");
        let t = ingest::add_file(&lib, &p).unwrap().track_id;
        std::fs::remove_file(&p).unwrap();
        let (r, events) = run(&c, &lib, t, &AtomicBool::new(false));
        assert!(r.is_err());
        assert_eq!(events.last(), Some(&Event::Failed { track_id: t, message: "The file was moved or deleted.".into() }));
        let s = lib.selected_source(t).unwrap().unwrap();
        assert_eq!((s.status, s.error.as_deref()), (SourceStatus::Failed, Some("The file was moved or deleted.")));
    }

    fn wait_for(rx: &mpsc::Receiver<Event>, want: &Event) -> Vec<Event> {
        let mut seen = Vec::new();
        loop {
            let e = rx.recv_timeout(Duration::from_secs(20)).expect("timed out waiting for event");
            let done = &e == want;
            seen.push(e);
            if done {
                return seen;
            }
        }
    }

    #[test]
    fn worker_prepares_the_queue_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let a = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        let b = ingest::add_file(&lib, &song(dir.path(), "b.wav")).unwrap().track_id;
        let (tx, rx) = mpsc::channel();
        let w = Worker::spawn(c, Box::new(Silence), Box::new(NoLyrics), tx);
        w.play(vec![a, b]);
        let seen = wait_for(&rx, &Event::Ready { track_id: b });
        let pos = |id| seen.iter().position(|e| e == &Event::Ready { track_id: id });
        assert!(pos(a) < pos(b));
    }

    #[test]
    fn worker_drops_a_song_when_you_switch_away() {
        struct Slow;
        impl VocalModel for Slow {
            fn infer(&mut self, i: ndarray::Array4<f32>) -> Result<ndarray::Array4<f32>> {
                std::thread::sleep(Duration::from_millis(2));
                Ok(ndarray::Array4::zeros(i.dim()))
            }
        }
        let dir = tempfile::tempdir().unwrap();
        let c = ctx(dir.path());
        let lib = Library::open(&c.store.db_path()).unwrap();
        let a = ingest::add_file(&lib, &song(dir.path(), "a.wav")).unwrap().track_id;
        let b = ingest::add_file(&lib, &song(dir.path(), "b.wav")).unwrap().track_id;
        let (tx, rx) = mpsc::channel();
        let w = Worker::spawn(c, Box::new(Slow), Box::new(NoLyrics), tx);
        w.play(vec![a]);
        wait_for(&rx, &Event::Stage { track_id: a, stage: Stage::Separating });
        w.play(vec![b]);
        let seen = wait_for(&rx, &Event::Ready { track_id: b });
        assert!(!seen.contains(&Event::Ready { track_id: a }));
    }
}
```

Add `pub mod jobs;` to `lib.rs`.

- [ ] **Step 2: Run tests to verify they fail**

Run: `cargo test -p kara-core jobs`
Expected: compile errors.

- [ ] **Step 3: Implement** — above the tests in `jobs.rs`:

```rust
//! Preparing a song for singing: fetch → standardize → (lyrics ‖ separate).
//! `Worker` runs one song at a time: the one you're about to sing first, then
//! the rest of the queue. Switching songs cancels the old one.

use crate::audio::{self, Stereo, SAMPLE_RATE};
use crate::cache;
use crate::ingest;
use crate::library::{AudioSource, Library, LyricsSource, SepStatus, SeparationRow, SourceKind, SourceStatus, Track};
use crate::lyrics::{self, LyricsFetcher};
use crate::now_ms;
use crate::separate::mdx::{self, MdxParams, Outcome, VocalModel};
use crate::store::{write_atomic, Store};
use anyhow::{Context, Result};
use std::collections::VecDeque;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc, Condvar, Mutex};
use std::thread::JoinHandle;

#[derive(Clone, Debug, PartialEq)]
pub enum Stage {
    Fetching,
    Standardizing,
    Lyrics,
    Separating,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Event {
    Stage { track_id: i64, stage: Stage },
    Progress { track_id: i64, chunks_done: u32, chunks_total: u32 },
    Ready { track_id: i64 },
    Failed { track_id: i64, message: String },
}

#[derive(Clone, Debug)]
pub struct Ctx {
    pub store: Store,
    pub model_id: String,
    pub params: MdxParams,
    pub chunk_len: usize,
}

/// "No lyrics" results are retried after a week.
const LYRICS_RETRY_MS: i64 = 7 * 24 * 3600 * 1000;

pub fn prepare(
    ctx: &Ctx,
    lib: &Library,
    model: &mut dyn VocalModel,
    fetcher: &(dyn LyricsFetcher + Sync),
    track_id: i64,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Event),
) -> Result<Outcome> {
    let result = prepare_inner(ctx, lib, model, fetcher, track_id, cancel, emit);
    if let Err(e) = &result {
        // The outermost context of every error on this path is written for the user.
        let message = e.to_string();
        if let Ok(Some(src)) = lib.selected_source(track_id) {
            let _ = lib.set_source_status(src.id, SourceStatus::Failed, Some(&message));
        }
        emit(Event::Failed { track_id, message });
    }
    result
}

fn is_ready(ctx: &Ctx, lib: &Library, hash: &str) -> Result<bool> {
    Ok(lib.separation(hash, &ctx.model_id)?.is_some_and(|r| r.status == SepStatus::Ready))
}

fn prepare_inner(
    ctx: &Ctx,
    lib: &Library,
    model: &mut dyn VocalModel,
    fetcher: &(dyn LyricsFetcher + Sync),
    track_id: i64,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Event),
) -> Result<Outcome> {
    let track = lib.track(track_id)?;
    let source = lib.selected_source(track_id)?.context("This song has no audio to play.")?;
    if let Some(hash) = &source.audio_hash {
        if is_ready(ctx, lib, hash)? {
            lib.touch_separation(hash, &ctx.model_id, now_ms())?;
            emit(Event::Ready { track_id });
            return Ok(Outcome::Done);
        }
    }

    let (hash, mix) = load_or_fetch(ctx, lib, &track, &source, emit)?;
    if is_ready(ctx, lib, &hash)? {
        lib.touch_separation(&hash, &ctx.model_id, now_ms())?;
        emit(Event::Ready { track_id });
        return Ok(Outcome::Done);
    }
    if cancel.load(Ordering::Relaxed) {
        return Ok(Outcome::Cancelled);
    }

    let track = lib.track(track_id)?; // fetching may have filled in title/artist
    let need_lyrics = match lib.lyrics(track_id)? {
        None => true,
        Some(l) => l.source == LyricsSource::None && now_ms() - l.fetched_at > LYRICS_RETRY_MS,
    };
    emit(Event::Stage { track_id, stage: Stage::Lyrics });
    std::thread::scope(|s| {
        let lookup = need_lyrics.then(|| {
            let track = &track;
            s.spawn(move || {
                let dur = track.duration_ms.unwrap_or(0);
                lyrics::find(None, &track.title, track.artist.as_deref(), track.album.as_deref(), dur, fetcher)
            })
        });
        let outcome = separate_stage(ctx, lib, track_id, &hash, &mix, model, cancel, emit);
        if let Some(handle) = lookup {
            // Offline or the service is down: leave lyrics unset so we try again next time.
            if let Ok(Ok((src, lines))) = handle.join() {
                lib.set_lyrics(track_id, src, &lines, now_ms())?;
            }
        }
        outcome
    })
}

/// The song in the standard format, plus its hash. Uses the stored copy when
/// there is one; otherwise fetches and converts the original.
fn load_or_fetch(ctx: &Ctx, lib: &Library, track: &Track, source: &AudioSource, emit: &mut dyn FnMut(Event)) -> Result<(String, Stereo)> {
    if let Some(hash) = &source.audio_hash {
        let p = ctx.store.source_path(hash);
        if p.exists() {
            return Ok((hash.clone(), audio::read_flac(&p).context("Couldn't read this song's audio.")?));
        }
    }
    emit(Event::Stage { track_id: track.id, stage: Stage::Fetching });
    lib.set_source_status(source.id, SourceStatus::Fetching, None)?;
    let path = ingest::fetch_audio(lib, &ctx.store, track, source)?;
    emit(Event::Stage { track_id: track.id, stage: Stage::Standardizing });
    let decoded = audio::decode_file(&path).context("Couldn't read this audio. The format may not be supported.")?;
    if source.kind == SourceKind::Link {
        let _ = std::fs::remove_file(&path);
    }
    anyhow::ensure!(!decoded.audio.is_empty(), "This audio is empty.");
    let hash = audio::audio_hash(&decoded.audio);
    let dst = ctx.store.source_path(&hash);
    if !is_ready(ctx, lib, &hash)? && !dst.exists() {
        write_atomic(&dst, &audio::encode_flac(&decoded.audio)?).context("Couldn't save the audio. Is the disk full?")?;
    }
    lib.set_source_audio(source.id, &hash, decoded.audio.duration_ms())?;
    if let Some(text) = decoded.tags.lyrics.as_deref().filter(|t| lyrics::is_synced(t)) {
        let lines = lyrics::parse_lrc(text, decoded.audio.duration_ms());
        lib.set_lyrics(track.id, LyricsSource::Embedded, &lines, now_ms())?;
    }
    Ok((hash, decoded.audio))
}

#[allow(clippy::too_many_arguments)]
fn separate_stage(
    ctx: &Ctx,
    lib: &Library,
    track_id: i64,
    hash: &str,
    mix: &Stereo,
    model: &mut dyn VocalModel,
    cancel: &AtomicBool,
    emit: &mut dyn FnMut(Event),
) -> Result<Outcome> {
    emit(Event::Stage { track_id, stage: Stage::Separating });
    let total = mdx::chunk_count(mix.len(), ctx.chunk_len) as u32;
    let start = cache::count_complete_chunks(&ctx.store, hash, &ctx.model_id);
    let mut row = lib.separation(hash, &ctx.model_id)?.unwrap_or(SeparationRow {
        audio_hash: hash.to_string(),
        model_id: ctx.model_id.clone(),
        chunk_ms: (ctx.chunk_len as u64 * 1000 / SAMPLE_RATE as u64) as u32,
        chunks_total: total,
        chunks_done: 0,
        status: SepStatus::Queued,
        size_bytes: 0,
        last_used_at: Some(now_ms()),
    });
    row.chunks_done = start;
    row.status = SepStatus::Running;
    lib.upsert_separation(&row)?;
    emit(Event::Progress { track_id, chunks_done: start, chunks_total: total });

    let outcome = mdx::separate(model, &ctx.params, mix, ctx.chunk_len, start as usize, cancel, |chunk| {
        cache::write_chunk(&ctx.store, hash, &ctx.model_id, &chunk)?;
        row.chunks_done = chunk.index as u32 + 1;
        lib.upsert_separation(&row)?;
        emit(Event::Progress { track_id, chunks_done: row.chunks_done, chunks_total: total });
        Ok(())
    })
    .context("Couldn't take the vocals out of this song.")?;

    match outcome {
        Outcome::Cancelled => {
            row.status = SepStatus::Cancelled;
            lib.upsert_separation(&row)?;
        }
        Outcome::Done => {
            cache::finish(&ctx.store, lib, hash, &ctx.model_id)?;
            emit(Event::Ready { track_id });
        }
    }
    Ok(outcome)
}

// ---------------------------------------------------------------- worker

struct State {
    queue: VecDeque<i64>,
    running: Option<(i64, Arc<AtomicBool>)>,
    shutdown: bool,
}

struct Shared {
    state: Mutex<State>,
    wake: Condvar,
}

pub struct Worker {
    shared: Arc<Shared>,
    handle: Option<JoinHandle<()>>,
}

impl Worker {
    pub fn spawn(ctx: Ctx, mut model: Box<dyn VocalModel>, fetcher: Box<dyn LyricsFetcher + Send + Sync>, events: mpsc::Sender<Event>) -> Self {
        let shared = Arc::new(Shared { state: Mutex::new(State { queue: VecDeque::new(), running: None, shutdown: false }), wake: Condvar::new() });
        let s = shared.clone();
        let handle = std::thread::spawn(move || {
            let lib = match Library::open(&ctx.store.db_path()) {
                Ok(l) => l,
                Err(_) => return,
            };
            loop {
                let (track, flag) = {
                    let mut st = s.state.lock().unwrap();
                    loop {
                        if st.shutdown {
                            return;
                        }
                        if let Some(t) = st.queue.pop_front() {
                            let f = Arc::new(AtomicBool::new(false));
                            st.running = Some((t, f.clone()));
                            break (t, f);
                        }
                        st = s.wake.wait(st).unwrap();
                    }
                };
                let _ = prepare(&ctx, &lib, model.as_mut(), fetcher.as_ref(), track, &flag, &mut |e| {
                    let _ = events.send(e);
                });
                let upcoming: Vec<i64> = {
                    let mut st = s.state.lock().unwrap();
                    st.running = None;
                    st.queue.iter().copied().collect()
                };
                let protected: Vec<String> = std::iter::once(track)
                    .chain(upcoming)
                    .filter_map(|t| lib.selected_source(t).ok().flatten()?.audio_hash)
                    .collect();
                let _ = cache::enforce_budget(&ctx.store, &lib, &protected);
            }
        });
        Self { shared, handle: Some(handle) }
    }

    /// `tracks[0]` is the song to prepare now; the rest are prepared next, in order.
    /// A running job for any other song is cancelled.
    pub fn play(&self, tracks: Vec<i64>) {
        let mut st = self.shared.state.lock().unwrap();
        let mut queue: VecDeque<i64> = tracks.into();
        if let Some((running, flag)) = &st.running {
            if queue.front() == Some(running) {
                queue.pop_front(); // already being prepared
            } else {
                flag.store(true, Ordering::Relaxed);
            }
        }
        st.queue = queue;
        self.shared.wake.notify_all();
    }
}

impl Drop for Worker {
    fn drop(&mut self) {
        {
            let mut st = self.shared.state.lock().unwrap();
            st.shutdown = true;
            if let Some((_, flag)) = &st.running {
                flag.store(true, Ordering::Relaxed);
            }
        }
        self.shared.wake.notify_all();
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }
}
```

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p kara-core jobs`
Expected: 6 passed.

- [ ] **Step 5: Run the whole suite**

Run: `cargo test -p kara-core`
Expected: all tests pass (1 ignored network test).

- [ ] **Step 6: Commit**

```bash
git add crates/kara-core
git commit -m "feat(core): prepare pipeline and cancellable worker queue"
```

---

### Task 11: CLI — add, prepare, search, export

**Files:**
- Modify: `crates/kara-cli/src/main.rs`, `crates/kara-cli/Cargo.toml`
- Create: `crates/kara-cli/tests/cli.rs`

**Interfaces:**
- Consumes: `ingest::{add_file, add_link, link::parse_link}`, `Library`, `Store`, `cache::{startup_cleanup, read_chunk}`, `jobs::{prepare, Ctx, Event, Stage}`, `lyrics::Lrclib`, `separate::{DEFAULT_MODEL, CHUNK_LEN, onnx::OnnxModel}`, `assets::ensure`.
- Produces: `kara add <path-or-link>`, `kara prepare <track-id> [--cpu]`, `kara search <query>`, `kara export <track-id> <out-dir>`, plus the existing `kara bench`.

- [ ] **Step 1: Write the failing integration test** — `crates/kara-cli/tests/cli.rs`:

```rust
use std::process::Command;

fn kara(data: &std::path::Path, args: &[&str]) -> (bool, String, String) {
    let out = Command::new(env!("CARGO_BIN_EXE_kara")).arg("--data").arg(data).args(args).output().unwrap();
    (out.status.success(), String::from_utf8_lossy(&out.stdout).into(), String::from_utf8_lossy(&out.stderr).into())
}

#[test]
fn add_then_search_a_file() {
    let dir = tempfile::tempdir().unwrap();
    let wav = dir.path().join("kitchen-light-waltz.wav");
    let spec = hound::WavSpec { channels: 2, sample_rate: 44_100, bits_per_sample: 16, sample_format: hound::SampleFormat::Int };
    let mut w = hound::WavWriter::create(&wav, spec).unwrap();
    for i in 0..44_100 {
        let s = ((i as f32 * 0.05).sin() * 8000.0) as i16;
        w.write_sample(s).unwrap();
        w.write_sample(s).unwrap();
    }
    w.finalize().unwrap();
    let data = dir.path().join("data");

    let (ok, out, _) = kara(&data, &["add", wav.to_str().unwrap()]);
    assert!(ok);
    assert!(out.contains("kitchen light waltz"), "{out}");

    let (ok, out, _) = kara(&data, &["search", "kitch"]);
    assert!(ok);
    assert!(out.contains("kitchen light waltz"), "{out}");
}

#[test]
fn streaming_links_are_refused_with_a_plain_message() {
    let dir = tempfile::tempdir().unwrap();
    let (ok, _, err) = kara(dir.path(), &["add", "https://open.spotify.com/track/abc"]);
    assert!(!ok);
    assert!(err.contains("can't be downloaded"), "{err}");
}
```

Add to `crates/kara-cli/Cargo.toml`:
```toml
[dev-dependencies]
hound = "3"
tempfile = "3"
```

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -p kara-cli`
Expected: FAIL (`add` / `search` are unknown subcommands).

- [ ] **Step 3: Implement** — in `main.rs`, extend `Cmd` and `main`, and add the handlers. Replace the `Cmd` enum and `main` with:

```rust
#[derive(Subcommand)]
enum Cmd {
    /// Add one audio file or one link to the library.
    Add { input: String },
    /// Get a song ready to sing (download, convert, lyrics, take the vocals out).
    Prepare {
        track_id: i64,
        #[arg(long)]
        cpu: bool,
    },
    /// Search the library.
    Search { query: String },
    /// Write a prepared song's vocals.wav and instrumental.wav, for listening.
    Export { track_id: i64, out: PathBuf },
    /// Measure separation speed and memory for one model on one song.
    Bench {
        input: PathBuf,
        #[arg(long)]
        model: PathBuf,
        #[arg(long, default_value_t = 1.0)]
        compensate: f32,
        #[arg(long)]
        cpu: bool,
        #[arg(long)]
        runtime: Option<PathBuf>,
        #[arg(long)]
        out: Option<PathBuf>,
    },
}

fn main() {
    if let Err(e) = run() {
        eprintln!("{e:#}");
        std::process::exit(1);
    }
}

fn run() -> Result<()> {
    let cli = Cli::parse();
    let store = Store::new(cli.data.clone().unwrap_or_else(Store::default_root));
    match cli.cmd {
        Cmd::Add { input } => add(&store, &input),
        Cmd::Prepare { track_id, cpu } => prepare(&store, track_id, !cpu),
        Cmd::Search { query } => search(&store, &query),
        Cmd::Export { track_id, out } => export(&store, track_id, &out),
        Cmd::Bench { input, model, compensate, cpu, runtime, out } => {
            bench(&store, &input, &model, compensate, !cpu, runtime, out.as_deref())
        }
    }
}
```

Add these imports at the top:
```rust
use anyhow::{bail, Context};
use kara_core::cache;
use kara_core::ingest::{self, link};
use kara_core::jobs::{self, Ctx, Event, Stage};
use kara_core::library::Library;
use kara_core::lyrics::Lrclib;
use kara_core::separate::DEFAULT_MODEL;
```

Add the handlers:
```rust
fn open_library(store: &Store) -> Result<Library> {
    let lib = Library::open(&store.db_path())?;
    cache::startup_cleanup(store, &lib)?;
    Ok(lib)
}

fn add(store: &Store, input: &str) -> Result<()> {
    let lib = open_library(store)?;
    let path = PathBuf::from(input);
    let ing = if path.exists() {
        ingest::add_file(&lib, &path)?
    } else {
        let url = link::parse_link(input).context("That's not a file or a link.")?;
        ingest::add_link(&lib, &url)?
    };
    let t = lib.track(ing.track_id)?;
    println!("{}\t{}", t.id, t.title);
    Ok(())
}

fn search(store: &Store, query: &str) -> Result<()> {
    let lib = open_library(store)?;
    for t in lib.search(query, 20)? {
        println!("{}\t{}\t{}", t.id, t.title, t.artist.unwrap_or_default());
    }
    Ok(())
}

fn prepare(store: &Store, track_id: i64, coreml: bool) -> Result<()> {
    let lib = open_library(store)?;
    let runtime = runtime_lib(store, None)?;
    let model_path = assets::ensure(&DEFAULT_MODEL.asset, &store.models_dir(), &mut |done, total| {
        if let Some(t) = total {
            eprint!("\rDownloading the vocal model… {}%", done * 100 / t.max(1));
        }
    })?;
    let mut model = OnnxModel::load(&runtime, &model_path, coreml)?;
    let ctx = Ctx { store: store.clone(), model_id: DEFAULT_MODEL.id.to_string(), params: DEFAULT_MODEL.params, chunk_len: CHUNK_LEN };
    let fetcher = Lrclib::new()?;
    let started = Instant::now();
    jobs::prepare(&ctx, &lib, &mut model, &fetcher, track_id, &AtomicBool::new(false), &mut |e| match e {
        Event::Stage { stage, .. } => eprintln!("{}", match stage {
            Stage::Fetching => "Getting the song…",
            Stage::Standardizing => "Preparing the audio…",
            Stage::Lyrics => "Finding the lyrics…",
            Stage::Separating => "Taking the vocals out…",
        }),
        Event::Progress { chunks_done, chunks_total, .. } => {
            eprintln!("  {chunks_done}/{chunks_total} ready  ({:.1} s)", started.elapsed().as_secs_f64())
        }
        Event::Ready { .. } => eprintln!("Ready to sing."),
        Event::Failed { message, .. } => eprintln!("{message}"),
    })?;
    let lyrics = lib.lyrics(track_id)?.map(|l| l.lines.len()).unwrap_or(0);
    println!("ready\t{}\tlyrics lines: {}", lib.track(track_id)?.title, lyrics);
    Ok(())
}

fn export(store: &Store, track_id: i64, out: &Path) -> Result<()> {
    let lib = open_library(store)?;
    let hash = lib.selected_source(track_id)?.and_then(|s| s.audio_hash).context("This song isn't prepared yet.")?;
    let row = lib.separation(&hash, DEFAULT_MODEL.id)?.context("This song isn't prepared yet.")?;
    if row.chunks_done == 0 {
        bail!("This song isn't prepared yet.");
    }
    let (mut vocals, mut inst) = (Stereo::default(), Stereo::default());
    for i in 0..row.chunks_done {
        let (v, n) = cache::read_chunk(store, &hash, DEFAULT_MODEL.id, i)?;
        vocals.append(&v);
        inst.append(&n);
    }
    std::fs::create_dir_all(out)?;
    write_wav(&out.join("vocals.wav"), &vocals)?;
    write_wav(&out.join("instrumental.wav"), &inst)?;
    println!("wrote {}", out.display());
    Ok(())
}
```

Remove the old `fn main() -> Result<()>` from Task 4 (replaced by `main` + `run` above).

- [ ] **Step 4: Run tests to verify they pass**

Run: `cargo test -p kara-cli`
Expected: 2 passed.

- [ ] **Step 5: End-to-end check with the real model** (manual, needs network)

```bash
cargo run --release -p kara-cli -- add "<one of your songs>.mp3"
cargo run --release -p kara-cli -- prepare <id printed above>
cargo run --release -p kara-cli -- export <id> /tmp/kara-e2e
cargo run --release -p kara-cli -- add "https://www.youtube.com/watch?v=<a song you own>"
cargo run --release -p kara-cli -- prepare <id>
```
Expected: stages print in order; "1/N ready" appears within a few seconds of "Taking the vocals out…"; "Ready to sing."; the YouTube song's title and artist are real (not "youtube.com link"); a known song reports lyrics lines > 0; `/tmp/kara-e2e/instrumental.wav` plays without vocals. Run `prepare` again on the same id — it prints "Ready to sing." immediately.

- [ ] **Step 6: Commit**

```bash
git add crates/kara-cli
git commit -m "feat(cli): add, prepare, search and export commands"
```

---

## Self-review notes (for the executor)

- Spec coverage in this plan: §2 search-bar link rules (Task 8), preparing stages (Task 10 events), §3.1 Local provider (Task 8), §3.2 processor (Tasks 1, 3, 7, 8, 10), §3.5 ML runtime (Task 4), §3.6 model spike (Task 5), §4 schema + files (Tasks 4, 6), §5 cache (Task 9), §6 error handling (Tasks 8, 10), §7 testing (all; `kara bench` in Task 4).
- Deliberately in Plan 1b, not here: Tauri shell and commands, binary chunk transfer to the web view, the Svelte UI, the Web Audio streamer (slider mix, key shift with signalsmith-stretch, lyric clock), the "Catching up…" behavior, and the All-view artist merge by name.
- Deliberately in later phases: phone mics (Phase 2), Spotify/Apple Music and match switching (Phase 3).
