//! Vocal separation: STFT, the MDX-Net runner and the ONNX Runtime model.

pub mod mdx;
pub mod stft;

/// Samples per channel in one stored chunk file (10 s at 44.1 kHz).
pub const CHUNK_LEN: usize = 441_000;
