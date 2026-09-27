//! kara-core: the karaoke engine. No Tauri here; the app and the CLI both call it.

pub mod assets;
pub mod audio;
pub mod cache;
pub mod fuzzy;
pub mod ingest;
pub mod jobs;
pub mod library;
pub mod lyric_sync;
pub mod lyrics;
pub mod problem;
pub mod quiet;
pub mod separate;
pub mod store;

#[cfg(test)]
pub(crate) mod test_util;

/// Milliseconds since the Unix epoch.
pub fn now_ms() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}
