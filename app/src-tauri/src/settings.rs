/// The system's preferred language tag, e.g. "ja-JP".
#[tauri::command]
pub fn system_locale() -> Option<String> {
    sys_locale::get_locale()
}
