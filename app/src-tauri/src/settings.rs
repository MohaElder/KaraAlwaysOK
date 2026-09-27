use crate::state::{AppError, AppState, Plain};
use kara_core::cache;
use serde::Serialize;
use tauri::State;

/// The system's preferred language tag, e.g. "ja-JP".
#[tauri::command]
pub fn system_locale() -> Option<String> {
    sys_locale::get_locale()
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct StorageInfo {
    used_bytes: u64,
    limit_bytes: u64,
}

fn storage(state: &AppState) -> anyhow::Result<StorageInfo> {
    Ok(StorageInfo { used_bytes: cache::usage(&state.store), limit_bytes: cache::budget(&state.lib.lock().unwrap())? })
}

#[tauri::command]
pub fn storage_info(state: State<'_, AppState>) -> Result<StorageInfo, AppError> {
    storage(state.inner()).plain()
}

#[tauri::command]
pub fn set_storage_limit(state: State<'_, AppState>, bytes: u64) -> Result<(), AppError> {
    state.lib.lock().unwrap().set_setting("cache_budget_bytes", &bytes.to_string()).plain()
}

/// Clears every prepared song except the one playing and the ones queued after it.
#[tauri::command]
pub fn clear_storage(state: State<'_, AppState>) -> Result<StorageInfo, AppError> {
    let protected = {
        let p = state.player.lock().unwrap();
        cache::audio_hashes(&state.lib.lock().unwrap(), p.upcoming())
    };
    cache::clear(&state.store, &state.lib.lock().unwrap(), &protected).plain()?;
    storage(state.inner()).plain()
}

/// Whether macOS is set to reduce transparency.
#[tauri::command]
pub fn reduce_transparency() -> bool {
    #[cfg(target_os = "macos")]
    {
        objc2_app_kit::NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceTransparency()
    }
    #[cfg(not(target_os = "macos"))]
    {
        false
    }
}
