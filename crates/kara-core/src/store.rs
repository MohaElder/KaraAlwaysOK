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
