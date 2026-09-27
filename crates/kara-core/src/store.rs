//! Where everything lives on disk (spec §4).

use anyhow::{bail, Context, Result};
use std::fs::{File, TryLockError};
use std::path::{Path, PathBuf};

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
        let unusable = "Couldn't open kara's data folder.";
        std::fs::create_dir_all(&self.root).context(unusable)?;
        let file = File::options().write(true).create(true).truncate(false).open(self.root.join("kara.lock")).context(unusable)?;
        match file.try_lock() {
            Ok(()) => Ok(DataLock { _file: file }),
            Err(TryLockError::WouldBlock) => bail!("kara is busy preparing another song."),
            Err(TryLockError::Error(e)) => Err(anyhow::Error::from(e).context(unusable)),
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
    /// The song's kept original file, `original.<ext>`, if there is one.
    pub fn original_path(&self, hash: &str) -> Option<PathBuf> {
        std::fs::read_dir(self.audio_dir(hash))
            .ok()?
            .filter_map(|e| Some(e.ok()?.path()))
            .find(|p| p.file_stem().is_some_and(|s| s == "original") && p.extension().is_none_or(|x| x != "part"))
    }
    /// Where to keep a song's original file with extension `ext`.
    pub fn original_dest(&self, hash: &str, ext: Option<&str>) -> PathBuf {
        self.audio_dir(hash).join(match ext {
            Some(ext) => format!("original.{ext}"),
            None => "original".to_string(),
        })
    }
    pub fn stems_dir(&self, hash: &str, model_id: &str) -> PathBuf {
        self.audio_dir(hash).join(model_id)
    }
    pub fn chunk_path(&self, hash: &str, model_id: &str, index: u32) -> PathBuf {
        self.stems_dir(hash, model_id).join(format!("{index:04}.flac"))
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
        assert_eq!(s.original_dest("abc", Some("m4a")), Path::new("/data/audio/abc/original.m4a"));
        assert_eq!(s.chunk_path("abc", "m1", 7), Path::new("/data/audio/abc/m1/0007.flac"));
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
    fn an_unusable_data_folder_gets_a_plain_message() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("data");
        std::fs::write(&root, b"a file, not a folder").unwrap();
        assert_eq!(Store::new(&root).lock().err().unwrap().to_string(), "Couldn't open kara's data folder.");
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
