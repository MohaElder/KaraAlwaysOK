//! Where everything lives on disk (spec §4).

use anyhow::{bail, Result};
use std::fs::{File, TryLockError};
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

/// Exclusive use of the data folder; released when dropped.
pub struct DataLock {
    _file: File,
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
    /// Takes the data folder for writing; fails with a plain message while another kara holds it.
    pub fn lock(&self) -> Result<DataLock> {
        std::fs::create_dir_all(&self.root)?;
        let file = File::options().write(true).create(true).truncate(false).open(self.root.join("kara.lock"))?;
        match file.try_lock() {
            Ok(()) => Ok(DataLock { _file: file }),
            Err(TryLockError::WouldBlock) => bail!("kara is busy preparing another song."),
            Err(TryLockError::Error(e)) => Err(e.into()),
        }
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
    fn only_one_writer_holds_the_data_folder() {
        let dir = tempfile::tempdir().unwrap();
        let s = Store::new(dir.path());
        let held = s.lock().unwrap();
        assert_eq!(s.lock().err().unwrap().to_string(), "kara is busy preparing another song.");
        drop(held);
        // A child process another test is starting can share the handle until it execs.
        let relocked = (0..200).any(|_| {
            let ok = s.lock().is_ok();
            if !ok {
                std::thread::sleep(std::time::Duration::from_millis(10));
            }
            ok
        });
        assert!(relocked);
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
