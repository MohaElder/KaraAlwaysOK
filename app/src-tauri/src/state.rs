use kara_core::cache;
use kara_core::jobs::{Adder, Ctx, Event, Worker};
use kara_core::library::Library;
use kara_core::lyrics::Lrclib;
use kara_core::problem::{problem, Problem};
use kara_core::separate::{CHUNK_LEN, DEFAULT_MODEL};
use kara_core::store::{DataLock, Store};
use serde::Serialize;
use std::sync::{mpsc, Arc, Mutex};

#[expect(dead_code, reason = "read by the library, adding and player commands")]
pub struct AppState {
    pub store: Store,
    pub lib: Mutex<Library>,
    /// Reads song audio.
    pub reader: Arc<Mutex<Library>>,
    /// The data folder lock, until the worker holds it.
    pub lock: Mutex<Option<DataLock>>,
    pub worker: Mutex<Option<Worker>>,
    /// Held while the engine is being set up.
    pub setup: tauri::async_runtime::Mutex<()>,
    pub adder: Adder,
    pub events: mpsc::Sender<Event>,
}

impl AppState {
    /// Takes the data folder for this run, tidies up after any crash and starts adding songs.
    pub fn open(events: mpsc::Sender<Event>) -> anyhow::Result<Self> {
        Self::try_open(events).map_err(|e| coded(e, Problem::LibraryOpen))
    }

    fn try_open(events: mpsc::Sender<Event>) -> anyhow::Result<Self> {
        let store = Store::new(Store::default_root());
        let lock = store.lock()?;
        let lib = Library::open(&store.db_path())?;
        cache::startup_cleanup(&store, &lib, &lock)?;
        let reader = Arc::new(Mutex::new(Library::open(&store.db_path())?));
        let adder = Adder::spawn(ctx(&store), Box::new(Lrclib::new()?), events.clone())?;
        Ok(Self { store, lib: Mutex::new(lib), reader, lock: Mutex::new(Some(lock)), worker: Mutex::new(None), setup: Default::default(), adder, events })
    }
}

pub fn ctx(store: &Store) -> Ctx {
    Ctx { store: store.clone(), model_id: DEFAULT_MODEL.id.to_string(), params: DEFAULT_MODEL.params, chunk_len: CHUNK_LEN }
}

/// `e` unchanged when it already names a problem (e.g. a library from a newer version), otherwise marked `fallback`.
pub fn coded(e: anyhow::Error, fallback: Problem) -> anyhow::Error {
    if problem(&e).is_some() {
        e
    } else {
        e.context(fallback)
    }
}

/// An error as the UI receives it: a problem code to show in the user's language, and the English text.
#[derive(Clone, Debug, Serialize)]
pub struct AppError {
    problem: Option<Problem>,
    message: String,
}

impl From<anyhow::Error> for AppError {
    fn from(e: anyhow::Error) -> Self {
        Self { problem: problem(&e), message: e.to_string() }
    }
}

impl From<tauri::Error> for AppError {
    fn from(e: tauri::Error) -> Self {
        anyhow::Error::from(e).into()
    }
}

/// Turns an engine error into what the UI gets.
pub trait Plain<T> {
    fn plain(self) -> Result<T, AppError>;
}

impl<T> Plain<T> for anyhow::Result<T> {
    fn plain(self) -> Result<T, AppError> {
        self.map_err(AppError::from)
    }
}
