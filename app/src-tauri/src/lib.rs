mod engine;
mod library;
mod player;
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
            player::player_state,
            player::queue_add,
            player::play_tracks,
            player::skip,
            player::song_ended,
            player::queue_move,
            player::queue_remove,
            player::set_singer,
            player::set_key,
            player::set_lyric_offset,
            player::retry_prepare,
            player::playback_info,
            player::chunk_pcm,
        ])
        .build(tauri::generate_context!())
        .expect("error while running KaraAlwaysOK")
        .run(|_, event| {
            if let tauri::RunEvent::Exit = event {
                kara_core::quiet::silence_output();
            }
        });
}
