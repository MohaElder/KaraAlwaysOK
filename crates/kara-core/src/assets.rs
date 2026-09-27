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
