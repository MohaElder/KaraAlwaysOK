mod settings;

pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![settings::system_locale])
        .run(tauri::generate_context!())
        .expect("error while running KaraAlwaysOK");
}
