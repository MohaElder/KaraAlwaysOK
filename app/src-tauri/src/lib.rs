mod adding;
mod engine;
mod library;
mod player;
mod phones;
mod settings;
mod state;

use kara_core::jobs::{Event, Stage};
use tauri::{Emitter, Manager};

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .setup(|app| {
            let (events, rx) = std::sync::mpsc::channel();
            let handle = app.handle().clone();
            std::thread::spawn(move || {
                for e in rx {
                    let _ = handle.emit("engine", &e);
                    if let Event::Ready { track_id } | Event::LyricOffset { track_id } | Event::Stage { track_id, stage: Stage::Separating } = e {
                        player::refresh_if_queued(&handle, track_id);
                    }
                    if let Event::Lyrics { track_id } = e {
                        phones::lyrics_changed(&handle, track_id);
                    }
                    phones::link_done(&handle, &e);
                }
            });
            app.manage(engine::Startup(events));
            let _ = engine::open_library(app.handle());
            app.manage(phones::Phones::default());
            #[cfg(debug_assertions)]
            {
                if let Ok(code) = std::env::var("KARA_PHONE_CODE") {
                    let handle = app.handle().clone();
                    tauri::async_runtime::spawn(async move {
                        let _ = phones::open(&handle, code).await;
                    });
                }
            }
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            settings::system_locale,
            settings::storage_info,
            settings::set_storage_limit,
            settings::clear_storage,
            settings::reduce_transparency,
            engine::startup_problem,
            engine::setup_engine,
            adding::search,
            adding::link_preview,
            adding::youtube_search,
            adding::youtube_suggestions,
            adding::bilibili_search,
            adding::bilibili_suggestions,
            adding::add_file,
            adding::add_link,
            adding::start_adding,
            library::delete_track,
            library::edit_track,
            library::list_collections,
            library::open_collection,
            library::get_track,
            library::track_lyrics,
            library::find_lyrics_again,
            library::create_playlist,
            library::rename_playlist,
            library::delete_playlist,
            library::add_to_playlist,
            library::remove_from_playlist,
            library::move_in_playlist,
            library::playlists_with,
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
            phones::phones_open,
            phones::phones_close,
            phones::phone_remove,
            phones::phone_volume,
            phones::phones_clock,
        ])
        .build(tauri::generate_context!())
        .expect("error while running KaraAlwaysOK")
        .run(|app, event| match event {
            tauri::RunEvent::ExitRequested { .. } => phones::end(app),
            tauri::RunEvent::Exit => kara_core::quiet::silence_output(),
            _ => {}
        });
}
