use crate::state::{coded, ctx, AppError, AppState, Plain};
use anyhow::Context;
use kara_core::assets;
use kara_core::jobs::{Event, Worker};
use kara_core::lyrics::Lrclib;
use kara_core::problem::Problem;
use kara_core::separate::mdx::VocalModel;
use kara_core::separate::onnx::OnnxModel;
use kara_core::separate::DEFAULT_MODEL;
use kara_core::store::Store;
use serde::Serialize;
use std::sync::mpsc;
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

/// Where the engine's events go, kept so the library can be opened again after a startup problem.
pub struct Startup(pub mpsc::Sender<Event>);

/// Opens the library and manages `AppState`, unless that is already done.
pub fn open_library(app: &AppHandle) -> Result<(), AppError> {
    if app.try_state::<AppState>().is_some() {
        return Ok(());
    }
    let state = AppState::open(app.state::<Startup>().0.clone())?;
    app.asset_protocol_scope().allow_directory(state.store.artwork_dir(), true)?;
    app.manage(state);
    Ok(())
}

/// Why the library can't be opened, trying again first; `None` once it is open.
#[tauri::command]
pub fn startup_problem(app: AppHandle) -> Option<AppError> {
    open_library(&app).err()
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SetupProgress {
    step: u8,
    steps: u8,
    done: u64,
    total: Option<u64>,
}

/// Downloads the singing engine on first run (reporting progress), loads the
/// model and starts preparing songs.
#[tauri::command]
pub async fn setup_engine(state: State<'_, AppState>, on_progress: Channel<SetupProgress>) -> Result<(), AppError> {
    let _once = state.setup.lock().await;
    if state.worker.lock().unwrap().is_some() {
        return Ok(());
    }
    let store = state.store.clone();
    let model = tauri::async_runtime::spawn_blocking(move || load_model(&store, &on_progress)).await?.plain()?;
    let lock = state.lock.lock().unwrap().take().map_or_else(|| state.store.lock(), Ok).plain()?;
    let (store, events) = (state.store.clone(), state.events.clone());
    let started = tauri::async_runtime::spawn_blocking(move || Worker::spawn(ctx(&store), lock, model, Box::new(Lrclib::new()?), events)).await;
    let worker = started.map_err(anyhow::Error::from).and_then(|r| r).map_err(|e| {
        *state.lock.lock().unwrap() = state.store.lock().ok();
        AppError::from(coded(e, Problem::EngineStart))
    })?;
    let player = state.player.lock().unwrap();
    worker.play(player.upcoming());
    *state.worker.lock().unwrap() = Some(worker);
    Ok(())
}

fn load_model(store: &Store, progress: &Channel<SetupProgress>) -> anyhow::Result<Box<dyn VocalModel>> {
    let report = |step: u8| {
        move |done: u64, total: Option<u64>| {
            let _ = progress.send(SetupProgress { step, steps: 2, done, total });
        }
    };
    let runtime = assets::ensure(&assets::RUNTIME, &store.runtime_dir(), &mut report(1)).context(Problem::EngineDownload)?;
    let model = assets::ensure(&DEFAULT_MODEL.asset, &store.models_dir(), &mut report(2)).context(Problem::EngineDownload)?;
    Ok(Box::new(OnnxModel::load(&runtime, &model, true).context(Problem::EngineStart)?))
}
