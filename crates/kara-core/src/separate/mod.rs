//! Vocal separation: STFT, the MDX-Net runner and the ONNX Runtime model.

pub mod mdx;
pub mod onnx;
pub mod stft;

use crate::assets::Asset;
use mdx::MdxParams;

/// Samples per channel in one stored chunk file (10 s at 44.1 kHz).
pub const CHUNK_LEN: usize = 441_000;

pub struct ModelSpec {
    /// Folder name for this model's chunk files, e.g. "kim-vocal-2".
    pub id: &'static str,
    pub asset: Asset,
    pub params: MdxParams,
}

/// Chosen in docs/superpowers/spikes/2026-09-26-model-choice.md.
pub const DEFAULT_MODEL: ModelSpec = ModelSpec {
    id: "kim-vocal-2",
    asset: Asset {
        file_name: "Kim_Vocal_2.onnx",
        url: "https://github.com/TRvlvr/model_repo/releases/download/all_public_uvr_models/Kim_Vocal_2.onnx",
        sha256: "ce74ef3b6a6024ce44211a07be9cf8bc6d87728cc852a68ab34eb8e58cde9c8b",
    },
    params: MdxParams { n_fft: 7680, hop: 1024, dim_f: 3072, dim_t: 256, compensate: 1.009 },
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
