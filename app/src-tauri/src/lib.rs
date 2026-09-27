mod engine;
mod library;
mod settings;
mod state;

use tauri::{Emitter, Manager};

pub fn run() {
    tauri::Builder::default()
        .setup(|app| {
            let (events, rx) = std::sync::mpsc::channel();
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                for e in rx {
                    let _ = handle.emit("engine", &e);
                }
            });
            app.manage(engine::Startup(events));
            let _ = engine::open_library(app.handle());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            settings::system_locale,
            engine::startup_problem,
            engine::setup_engine,
            library::list_collections,
            library::open_collection,
            library::get_track,
            library::track_lyrics,
        ])
        .build(tauri::generate_context!())
        .expect("error while running KaraAlwaysOK")
        .run(|_, event| {
            if let tauri::RunEvent::Exit = event {
                kara_core::quiet::silence_output();
            }
        });
}
