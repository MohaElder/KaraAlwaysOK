//! kara-core: the karaoke engine. No Tauri here; the app and the CLI both call it.

pub mod assets;
pub mod audio;
pub mod ingest;
pub mod library;
pub mod lyrics;
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
