//! Phone mics: the session guests join, the Mac's commands for it, and what phones and the Mac window hear.

mod cert;
mod output;
mod room;
mod server;

use crate::adding::{self, SearchOutcome, WebSource};
use crate::library::{self, Lyrics};
use crate::player::{self, PlayerSnapshot};
use crate::state::{AppError, AppState, Plain};
use anyhow::Context;
use axum_server::tls_rustls::RustlsConfig;
use kara_core::ingest::preview::{LinkPreview, SearchHit};
use kara_core::jobs::Event;
use kara_core::library::{CollectionKind, Library};
use kara_core::mic::{voice_gain, Level, Mixer, NewVoice, MOST_PHONES};
use kara_core::problem::Problem;
use qrcode::render::svg;
use qrcode::QrCode;
use room::{Guest, Out, Room, ENDED};
use serde::Serialize;
use serde_json::Value;
use server::FromPhone;
use std::net::IpAddr;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc::UnboundedSender;

static NEXT: AtomicU64 = AtomicU64::new(1);
const MAX_QUERY: usize = 200;
const MAX_URL: usize = 2048;
const MAX_LINKS: usize = 3;
const MAX_LOOKUPS: usize = 6;

/// The phone session while there is one; `opening` keeps two opens from starting two servers.
#[derive(Default)]
pub struct Phones {
    session: Mutex<Option<Session>>,
    opening: tauri::async_runtime::Mutex<()>,
}

struct Session {
    id: u64,
    room: Room,
    window_open: bool,
    join: JoinInfo,
    server: axum_server::Handle,
    /// The view the Mac window last heard.
    shown: Option<PhonesView>,
    clock: Clock,
    links: Vec<PendingLink>,
    /// One entry per web lookup running for a phone, by its id.
    lookups: Vec<String>,
    mixer: Arc<Mutex<Mixer>>,
    /// The mix's output rate and jitter buffer floor, fixed for the session.
    rate: u32,
    floor_ms: f64,
    _output: output::Output,
    levels: Vec<Level>,
    tls: RustlsConfig,
    dir: PathBuf,
    host: String,
    port: u16,
    redirect: bool,
    ips: Vec<IpAddr>,
}

/// A song a guest added by link, queued once it has been downloaded.
struct PendingLink {
    track_id: i64,
    guest: String,
    name: String,
    next: bool,
    /// The link made this song, so it leaves the library again if it fails.
    fresh: bool,
}

/// Where the Mac last said the song was, and when.
struct Clock {
    key: Option<u64>,
    position_ms: i64,
    playing: bool,
    at: Instant,
}

impl Clock {
    fn new(key: Option<u64>, position_ms: i64, playing: bool) -> Self {
        Self { key, position_ms, playing, at: Instant::now() }
    }

    /// Where the song is now, as a message for phones.
    fn message(&self) -> ToPhone<'static> {
        let moved = if self.playing { self.at.elapsed().as_millis() as i64 } else { 0 };
        ToPhone::Clock { key: self.key, position_ms: self.position_ms + moved, playing: self.playing }
    }
}

#[derive(Clone, PartialEq, Serialize)]
pub struct JoinInfo {
    qr: String,
    code: String,
    host: String,
}

#[derive(Clone, PartialEq, Serialize)]
pub struct PhoneRow {
    id: String,
    name: String,
    connected: bool,
    volume: u8,
}

#[derive(Clone, Default, PartialEq, Serialize)]
pub struct PhonesView {
    join: Option<JoinInfo>,
    phones: Vec<PhoneRow>,
}

/// What the Mac toasts about phones.
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum News {
    Joined { name: String, mic: usize },
    Added { name: String, title: String },
}

/// A phone's level as the Mac window's meter reads it.
#[derive(Serialize)]
pub struct PhoneLevel {
    id: String,
    level: f32,
    down: bool,
}

/// What the Mac tells a phone.
#[derive(Serialize)]
#[serde(tag = "t", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum ToPhone<'a> {
    Joined,
    Player { snapshot: &'a PlayerSnapshot },
    Clock { key: Option<u64>, position_ms: i64, playing: bool },
    Lyrics { track_id: i64, lyrics: &'a Lyrics },
    LyricsChanged { track_id: i64 },
    Results { q: &'a str, outcome: &'a SearchOutcome },
    Web { source: WebSource, q: &'a str, hits: &'a [SearchHit] },
    Preview { url: &'a str, preview: Option<&'a LinkPreview> },
    Refused { problem: Option<Problem> },
    Level { v: f32 },
}

impl Session {
    fn view(&self) -> PhonesView {
        let phones = self.room.guests.iter().map(|g| PhoneRow { id: g.id.clone(), name: g.name.clone(), connected: g.tx.is_some(), volume: g.volume }).collect();
        PhonesView { join: Some(self.join.clone()), phones }
    }
}

/// Starts a session with `code`: certificate, sound output, server, join details.
async fn start(app: &AppHandle, code: &str) -> anyhow::Result<Session> {
    let ips = cert::lan_ips();
    let ip = *ips.first().context(Problem::NoNetwork)?;
    let host = cert::local_name().unwrap_or_else(|| ip.to_string());
    let dir = app.try_state::<AppState>().context(Problem::LibraryOpen)?.store.root().join("phones");
    let (cert_pem, key_pem) = cert::ensure(&dir, &host, &ips, kara_core::now_ms() / 1000).context(Problem::PhonesStart)?;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let tls = RustlsConfig::from_pem(cert_pem, key_pem).await.context(Problem::PhonesStart)?;
    let chosen = if cfg!(debug_assertions) { std::env::var("KARA_MIC_BUFFER_MS").ok().and_then(|v| v.parse::<f64>().ok()) } else { None };
    let floor_ms = chosen.unwrap_or(10.0).clamp(10.0, 60.0);
    let (output, mixer) = tauri::async_runtime::spawn_blocking(output::start).await?.context(Problem::PhonesStart)?;
    let rate = mixer.lock().unwrap().rate();
    let server = axum_server::Handle::new();
    let (port, redirect) = server::serve(app.clone(), tls.clone(), server.clone()).context(Problem::PhonesStart)?;
    let room = Room::new(code);
    let join = join_info(&host, ip, port, redirect, &room.code).context(Problem::PhonesStart)?;
    Ok(Session {
        id: NEXT.fetch_add(1, Ordering::Relaxed),
        room,
        window_open: false,
        join,
        server,
        shown: None,
        clock: Clock::new(None, 0, false),
        links: Vec::new(),
        lookups: Vec::new(),
        mixer,
        rate,
        floor_ms,
        _output: output,
        levels: Vec::with_capacity(MOST_PHONES),
        tls,
        dir,
        host,
        port,
        redirect,
        ips,
    })
}

/// What the Mac window shows to join: a QR code of the address with the code, the code, and the address to type
/// (the name alone while the port-80 redirect runs, the full address otherwise).
fn join_info(host: &str, ip: IpAddr, port: u16, redirect: bool, code: &str) -> anyhow::Result<JoinInfo> {
    let at = |h: &str| if port == 443 { h.to_string() } else { format!("{h}:{port}") };
    let url = format!("https://{}/phone?code={code}", at(&ip.to_string()));
    let qr = QrCode::new(url.as_bytes())?
        .render::<svg::Color>()
        .min_dimensions(176, 176)
        .quiet_zone(false)
        .dark_color(svg::Color("currentColor"))
        .light_color(svg::Color("transparent"))
        .build();
    let host = if redirect { host.to_string() } else { format!("https://{}", at(host)) };
    Ok(JoinInfo { qr, code: format!("OKI-{code}"), host })
}

/// Opens a session with `code` unless one is running.
pub async fn ensure_session(app: &AppHandle, code: String) -> anyhow::Result<()> {
    let phones = app.state::<Phones>();
    let _once = phones.opening.lock().await;
    if phones.session.lock().unwrap().is_some() {
        return Ok(());
    }
    let session = start(app, &code).await?;
    let id = session.id;
    *phones.session.lock().unwrap() = Some(session);
    tauri::async_runtime::spawn(tick(app.clone(), id));
    Ok(())
}

/// Runs `f` on the session, if there is one.
fn with<T>(app: &AppHandle, f: impl FnOnce(&mut Session) -> T) -> Option<T> {
    let phones = app.try_state::<Phones>()?;
    let mut slot = phones.session.lock().unwrap();
    slot.as_mut().map(f)
}

/// Tells the Mac window who is here now, if that changed.
fn changed(app: &AppHandle) {
    let view = with(app, |s| {
        let view = s.view();
        (s.shown.as_ref() != Some(&view)).then(|| s.shown.insert(view).clone())
    });
    if let Some(Some(view)) = view {
        let _ = app.emit("phones", view);
    }
}

/// Ends the session: every phone hears it ended and the server stops.
pub fn end(app: &AppHandle) {
    let Some(s) = app.try_state::<Phones>().and_then(|p| p.session.lock().unwrap().take()) else { return };
    for g in &s.room.guests {
        g.send(Out::Close(ENDED));
    }
    s.server.graceful_shutdown(Some(Duration::from_secs(1)));
    let _ = app.emit("phones", PhonesView::default());
}

/// Opens the window's session (starting one with `code` if none runs) and returns what the window shows.
pub async fn open(app: &AppHandle, code: String) -> anyhow::Result<PhonesView> {
    ensure_session(app, code).await?;
    Ok(with(app, |s| {
        s.window_open = true;
        s.view()
    })
    .unwrap_or_default())
}

#[tauri::command]
pub async fn phones_open(app: AppHandle) -> Result<PhonesView, AppError> {
    let code = room::new_code().context(Problem::PhonesStart).plain()?;
    open(&app, code).await.plain()
}

#[tauri::command]
pub fn phones_close(app: AppHandle) {
    let idle = with(&app, |s| {
        s.window_open = false;
        s.room.guests.is_empty()
    });
    if idle == Some(true) {
        end(&app);
    }
}

#[tauri::command]
pub fn phone_remove(app: AppHandle, id: String) {
    with(&app, |s| {
        if let Some(g) = s.room.kick(&id) {
            g.send(Out::Close(ENDED));
        }
        let gone = s.mixer.lock().unwrap().remove(&id);
        drop(gone);
    });
    changed(&app);
}

/// Lets a phone in, handing back its connection number and the mix its sound goes to, or names the close code refusing it; a
/// phone let in hears it joined, what's playing and where the song is, before any later change (the queue stays locked until then).
pub(crate) fn admit(app: &AppHandle, code: &str, id: &str, name: &str, tx: UnboundedSender<Out>) -> Result<(u64, Arc<Mutex<Mixer>>), u16> {
    let conn = NEXT.fetch_add(1, Ordering::Relaxed);
    let state = app.state::<AppState>();
    let queue = state.player.lock().unwrap();
    let lib = state.lib.lock().unwrap();
    let (joined, mixer) = with(app, |s| {
        let new = s.room.admit(code, id, name, conn, tx.clone(), kara_core::now_ms())?;
        let _ = tx.send(Out::Text(encode(&ToPhone::Joined)));
        if let Ok(snapshot) = player::snapshot(&lib, &queue) {
            let _ = tx.send(Out::Text(encode(&ToPhone::Player { snapshot: &snapshot })));
        }
        let _ = tx.send(Out::Text(encode(&s.clock.message())));
        let g = &s.room.guests;
        Ok((new.then(|| (g.len(), g[g.len() - 1].name.clone())), s.mixer.clone()))
    })
    .ok_or(ENDED)?
    .map_err(|r: room::Refusal| r.close_code())?;
    drop((lib, queue));
    changed(app);
    if let Some((mic, name)) = joined {
        let _ = app.emit("phone-news", News::Joined { name, mic });
    }
    Ok((conn, mixer))
}

/// Carries out what phone `id` asked for; songs it adds carry the guest's name. A refusal goes back to that phone.
pub(crate) fn handle(app: &AppHandle, id: &str, msg: FromPhone) {
    let Some(name) = with(app, |s| s.room.guest(id).map(|g| g.name.clone())).flatten() else { return };
    let reply = match msg {
        FromPhone::Singer { v } => player::set_singer(app.clone(), app.state(), v).map(|_| None),
        FromPhone::Add { track_id, next } => queue_for(app, track_id, next, &name).map(|()| None),
        FromPhone::AddLink { url, next } => add_link(app, id, &name, url, next).map(|()| None),
        FromPhone::Move { key, to } => player::queue_move(app.clone(), app.state(), key, to).map(|_| None),
        FromPhone::Remove { key } => player::queue_remove(app.clone(), app.state(), key).map(|_| None),
        FromPhone::Search { q, imported } => {
            let (q, imported) = (short(&q), short(&imported));
            let found = phone_search(&app.state::<AppState>().lib.lock().unwrap(), &q, &imported);
            found.plain().map(|outcome| Some(encode(&ToPhone::Results { q: &q, outcome: &outcome })))
        }
        FromPhone::Web { source, q } => {
            let q = short(&q);
            let none = encode(&ToPhone::Web { source, q: &q, hits: &[] });
            later(app, id, none, move |bin_dir, answer| answer(encode(&ToPhone::Web { source, q: &q, hits: &adding::find_on_web(bin_dir, source, &q) })));
            Ok(None)
        }
        FromPhone::Preview { url } => {
            let url: String = url.chars().take(MAX_URL).collect();
            let none = encode(&ToPhone::Preview { url: &url, preview: None });
            later(app, id, none.clone(), move |bin_dir, answer| {
                if adding::preview_link(bin_dir, &url, |p| answer(encode(&ToPhone::Preview { url: &url, preview: Some(&p) }))).is_err() {
                    answer(none);
                }
            });
            Ok(None)
        }
        FromPhone::Lyrics { track_id } => library::track_lyrics(app.state(), track_id).map(|lyrics| Some(encode(&ToPhone::Lyrics { track_id, lyrics: &lyrics }))),
        FromPhone::Live { on, rate } => {
            live(app, id, on, rate);
            Ok(None)
        }
        FromPhone::Voice { v } => {
            set_gain(app, id, |g| g.voice = v.min(100));
            Ok(None)
        }
        FromPhone::Effect { kind, amount } => {
            with(app, |s| {
                if let Some(g) = s.room.guest(id) {
                    g.effect = (kind, amount.min(100));
                    s.mixer.lock().unwrap().set_effect(id, kind, amount);
                }
            });
            Ok(None)
        }
        FromPhone::Join { .. } | FromPhone::Ping | FromPhone::Leave => Ok(None),
    };
    match reply {
        Ok(Some(text)) => tell(app, id, text),
        Ok(None) => {}
        Err(e) => tell(app, id, encode(&ToPhone::Refused { problem: e.problem })),
    }
}

/// A phone's mic went live at `rate` (a fresh buffer, built before taking the mixer lock, with the row's gain and effect) or was
/// muted (its buffer empties).
fn live(app: &AppHandle, id: &str, on: bool, rate: u32) {
    if !(8_000..=96_000).contains(&rate) {
        return;
    }
    with(app, |s| {
        let Some(g) = s.room.guest(id) else { return };
        let (gain, (effect, amount)) = (voice_gain(g.volume, g.voice), g.effect);
        if !on {
            return s.mixer.lock().unwrap().reset(id);
        }
        let voice = NewVoice::new(id, rate, s.rate, s.floor_ms);
        let leftover = {
            let mut m = s.mixer.lock().unwrap();
            let leftover = m.add(voice);
            m.set_gain(id, gain);
            m.set_effect(id, effect, amount);
            leftover
        };
        drop(leftover);
    });
}

/// Changes a phone's row with `change` and applies its gain to the mix.
fn set_gain(app: &AppHandle, id: &str, change: impl FnOnce(&mut Guest)) {
    with(app, |s| {
        if let Some(g) = s.room.guest(id) {
            change(g);
            let gain = voice_gain(g.volume, g.voice);
            s.mixer.lock().unwrap().set_gain(id, gain);
        }
    });
}

/// Adds a phone's 16-bit little-endian samples to its buffer.
pub(crate) fn hear(mixer: &Mutex<Mixer>, id: &str, bytes: &[u8]) {
    let samples: Vec<f32> = bytes.chunks_exact(2).map(|b| f32::from(i16::from_le_bytes([b[0], b[1]])) / 32_768.0).collect();
    mixer.lock().unwrap().push(id, &samples);
}

#[tauri::command]
pub fn phone_volume(app: AppHandle, id: String, volume: u8) {
    set_gain(&app, &id, |g| g.volume = volume.min(100));
    changed(&app);
}

/// Runs a slow web lookup for phone `id` away from its connection, handing `work` the tools folder and a way to answer the
/// phone; a phone with `MAX_LOOKUPS` already running is answered `busy` at once.
fn later(app: &AppHandle, id: &str, busy: String, work: impl FnOnce(&Path, &dyn Fn(String)) + Send + 'static) {
    let started = with(app, |s| {
        let free = s.lookups.iter().filter(|g| *g == id).count() < MAX_LOOKUPS;
        if free {
            s.lookups.push(id.to_string());
        }
        free
    });
    if started != Some(true) {
        return tell(app, id, busy);
    }
    let (app, id, bin_dir) = (app.clone(), id.to_string(), app.state::<AppState>().store.bin_dir());
    tauri::async_runtime::spawn_blocking(move || {
        work(&bin_dir, &|text| tell(&app, &id, text));
        with(&app, |s| s.lookups.iter().position(|g| *g == id).map(|i| s.lookups.remove(i)));
    });
}

/// The first `MAX_QUERY` characters of `s`.
fn short(s: &str) -> String {
    s.chars().take(MAX_QUERY).collect()
}

/// Sends `text` to one phone.
fn tell(app: &AppHandle, id: &str, text: String) {
    with(app, |s| {
        if let Some(g) = s.room.guest(id) {
            g.send(Out::Text(text));
        }
    });
}

/// Queues a song for guest `name` and tells the Mac window who added what.
fn queue_for(app: &AppHandle, track_id: i64, next: bool, name: &str) -> Result<(), AppError> {
    let snap = player::queue_add(app.clone(), app.state(), track_id, next, Some(name.to_string()))?;
    let title = snap.entries.iter().rev().find(|e| e.track.id == track_id).map(|e| e.track.title.clone()).unwrap_or_default();
    let _ = app.emit("phone-news", News::Added { name: name.to_string(), title });
    Ok(())
}

/// Adds a guest's link like the Mac does: queued once it's downloaded, or right away when it needs nothing more. A guest waits
/// while three of their links are still being added; a link the guest is already adding is ignored.
fn add_link(app: &AppHandle, id: &str, name: &str, url: String, next: bool) -> Result<(), AppError> {
    if with(app, |s| s.links.iter().filter(|l| l.guest == id).count()).unwrap_or(0) >= MAX_LINKS {
        return Err(anyhow::Error::new(Problem::TooManyLinks).into());
    }
    let added = adding::ingest_link(&app.state::<AppState>().lib.lock().unwrap(), &url)?;
    if with(app, |s| s.links.iter().any(|l| l.guest == id && l.track_id == added.track_id)) == Some(true) {
        return Ok(());
    }
    let _ = app.emit("library", ());
    if !adding::start_adding(app.state(), added.track_id)? {
        return queue_for(app, added.track_id, next, name);
    }
    with(app, |s| s.links.push(PendingLink { track_id: added.track_id, guest: id.to_string(), name: name.to_string(), next, fresh: added.new }));
    Ok(())
}

/// The engine finished adding a song: a guest's link is queued now, or on failure the guest hears why and a song the link made
/// leaves the library again unless its audio was fetched.
pub fn link_done(app: &AppHandle, e: &Event) {
    let (Event::Added { track_id } | Event::Failed { track_id, .. }) = e else { return };
    let Some(link) = with(app, |s| s.links.iter().position(|l| l.track_id == *track_id).map(|i| s.links.remove(i))).flatten() else { return };
    let refused = match e {
        Event::Failed { problem, .. } => {
            let fetched = adding::has_audio(&app.state::<AppState>().lib.lock().unwrap(), link.track_id).unwrap_or(true);
            if link.fresh && !fetched {
                let _ = library::delete_track(app.clone(), app.state(), link.track_id);
                let _ = app.emit("library", ());
            }
            Some(*problem)
        }
        _ => queue_for(app, link.track_id, link.next, &link.name).err().map(|e| e.problem),
    };
    if let Some(problem) = refused {
        tell(app, &link.guest, encode(&ToPhone::Refused { problem }));
    }
}

/// What a phone's search shows: every song by title when it is empty, otherwise the Mac's search.
fn phone_search(lib: &Library, q: &str, imported: &str) -> anyhow::Result<SearchOutcome> {
    if !q.trim().is_empty() {
        return adding::search_input(lib, q, imported);
    }
    let mut tracks = Vec::new();
    for c in lib.collections(None, CollectionKind::Playlist)?.into_iter().filter(|c| !c.user) {
        tracks.extend(lib.collection_tracks(c.id)?);
    }
    tracks.sort_by_key(|t| t.title.to_lowercase());
    Ok(SearchOutcome::Text { tracks, collections: Vec::new() })
}

/// Sends one message to every connected phone.
fn broadcast(app: &AppHandle, msg: &ToPhone) {
    with(app, |s| send_all(s, msg));
}

fn send_all(s: &Session, msg: &ToPhone) {
    let text = encode(msg);
    for g in &s.room.guests {
        g.send(Out::Text(text.clone()));
    }
}

/// Sends the queue to every phone.
pub fn send_player(app: &AppHandle, snapshot: &PlayerSnapshot) {
    broadcast(app, &ToPhone::Player { snapshot });
}

/// Tells phones a song's lyrics changed, so a phone showing it asks again.
pub fn lyrics_changed(app: &AppHandle, track_id: i64) {
    broadcast(app, &ToPhone::LyricsChanged { track_id });
}

/// The Mac says where the song is; phones follow it for the lyrics.
#[tauri::command]
pub fn phones_clock(app: AppHandle, key: Option<u64>, position_ms: i64, playing: bool) {
    with(&app, |s| {
        s.clock = Clock::new(key, position_ms, playing);
        send_all(s, &s.clock.message());
    });
}

/// A phone left: its row goes, and the session ends when nobody is left and the window is closed.
pub(crate) fn leave(app: &AppHandle, id: &str) {
    let idle = with(app, |s| {
        s.room.remove(id);
        let gone = s.mixer.lock().unwrap().remove(id);
        drop(gone);
        !s.window_open && s.room.guests.is_empty()
    });
    if idle == Some(true) {
        end(app);
    } else {
        changed(app);
    }
}

/// A phone's connection closed; its row stays, greyed, until it comes back or is removed.
pub(crate) fn dropped(app: &AppHandle, id: &str, conn: u64) {
    with(app, |s| s.room.dropped(id, conn, Instant::now()));
    changed(app);
}

/// What one tick found.
struct Tick {
    /// Each phone's level, while any phone is in the room.
    levels: Option<Vec<PhoneLevel>>,
    expired: bool,
    idle: bool,
    moved: bool,
}

impl Session {
    /// One tick: each phone hears its level, rows gone for two minutes leave, and every 60th tick the Mac's addresses are checked.
    fn tick(&mut self, n: u64, now: Instant) -> Tick {
        self.levels.clear();
        self.mixer.lock().unwrap().levels(&mut self.levels);
        for g in &self.room.guests {
            let v = self.levels.iter().find(|l| *l.id == *g.id).map_or(0.0, |l| l.peak);
            g.send(Out::Text(encode(&ToPhone::Level { v })));
        }
        let gone = self.room.expire(now);
        for g in &gone {
            let voice = self.mixer.lock().unwrap().remove(&g.id);
            drop(voice);
        }
        Tick {
            levels: (!self.room.guests.is_empty()).then(|| self.levels.iter().map(|l| PhoneLevel { id: l.id.to_string(), level: l.peak, down: l.down }).collect()),
            expired: !gone.is_empty(),
            idle: !gone.is_empty() && !self.window_open && self.room.guests.is_empty(),
            moved: n.is_multiple_of(60) && cert::lan_ips() != self.ips,
        }
    }
}

/// Every 80 ms while this session lasts: levels, gone phones, a new address now and then; the session ends when the Mac wakes
/// from sleep or its last phone is gone with the window closed.
async fn tick(app: AppHandle, id: u64) {
    let mut every = tokio::time::interval(Duration::from_millis(80));
    let mut last = (SystemTime::now(), Instant::now());
    for n in 1u64.. {
        every.tick().await;
        let now = (SystemTime::now(), Instant::now());
        let slept = woke(now.0.duration_since(last.0).unwrap_or_default(), now.1 - last.1);
        last = now;
        let Some(t) = with(&app, |s| (s.id == id).then(|| s.tick(n, now.1))).flatten() else { return };
        if slept || t.idle {
            return end(&app);
        }
        if let Some(levels) = &t.levels {
            let _ = app.emit("phone-levels", levels);
        }
        if t.expired {
            changed(&app);
        }
        if t.moved {
            readdress(&app, id).await;
        }
    }
}

/// Whether the wall clock ran far ahead of the uptime clock between two ticks, as it does across sleep.
fn woke(wall: Duration, uptime: Duration) -> bool {
    wall > uptime + Duration::from_secs(10)
}

/// Makes a certificate for the Mac's new addresses, loads it into the running server and points the QR code at the new address.
async fn readdress(app: &AppHandle, id: u64) {
    let ips = cert::lan_ips();
    let Some(&ip) = ips.first() else { return };
    let Some((dir, host, port, redirect, code, tls)) = with(app, |s| (s.id == id).then(|| (s.dir.clone(), s.host.clone(), s.port, s.redirect, s.room.code.clone(), s.tls.clone()))).flatten() else { return };
    let (named, covered) = (host.clone(), ips.clone());
    let made = tauri::async_runtime::spawn_blocking(move || cert::ensure(&dir, &named, &covered, kara_core::now_ms() / 1000)).await;
    let Ok(Ok((cert, key))) = made else { return };
    if tls.reload_from_pem(cert, key).await.is_err() {
        return;
    }
    let Ok(join) = join_info(&host, ip, port, redirect, &code) else { return };
    with(app, |s| {
        if s.id == id {
            s.ips = ips;
            s.join = join;
        }
    });
    changed(app);
}

/// A message as a phone reads it: JSON whose song pictures point at this server.
pub(crate) fn encode(msg: &ToPhone) -> String {
    let mut v = serde_json::to_value(msg).unwrap_or_default();
    art_links(&mut v);
    v.to_string()
}

fn art_links(v: &mut Value) {
    match v {
        Value::Object(map) => {
            for (k, x) in map.iter_mut() {
                match x {
                    Value::String(path) if k == "artworkPath" => {
                        let name = std::path::Path::new(path.as_str()).file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                        *path = format!("/art/{name}");
                    }
                    _ => art_links(x),
                }
            }
        }
        Value::Array(items) => items.iter_mut().for_each(art_links),
        _ => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::player::{snapshot, Player};
    use kara_core::library::{Library, NewTrack, ProviderId};

    #[test]
    fn song_pictures_reach_phones_as_links_to_this_server() {
        let lib = Library::open_in_memory().unwrap();
        let t = lib.add_track(&NewTrack { provider: ProviderId::Local, provider_ref: None, title: "Paper Boats", artist: None, album: None, duration_ms: None }).unwrap();
        lib.set_artwork(t, "/Users/someone/Library/Application Support/kara-always-oki/artwork/ab12.jpg").unwrap();
        let mut p = Player::default();
        p.add(t, false, None);
        let json: Value = serde_json::from_str(&encode(&ToPhone::Player { snapshot: &snapshot(&lib, &p).unwrap() })).unwrap();
        assert_eq!((json["t"].as_str(), json["snapshot"]["entries"][0]["track"]["artworkPath"].as_str()), (Some("player"), Some("/art/ab12.jpg")));
    }

    #[test]
    fn an_empty_phone_search_lists_every_song_by_title() {
        let lib = Library::open_in_memory().unwrap();
        let imported = lib.upsert_collection(ProviderId::Local, CollectionKind::Playlist, "imported", "Imported", None).unwrap();
        for title in ["lemon Skies", "Paper Boats", "Kettle Duet"] {
            let t = lib.add_track(&NewTrack { provider: ProviderId::Local, provider_ref: None, title, artist: None, album: None, duration_ms: None }).unwrap();
            lib.add_to_collection(imported, t).unwrap();
        }
        let titles = |q: &str| match phone_search(&lib, q, "Imported").unwrap() {
            SearchOutcome::Text { tracks, .. } => tracks.into_iter().map(|t| t.title).collect::<Vec<_>>(),
            _ => Vec::new(),
        };
        assert_eq!(titles(" "), ["Kettle Duet", "lemon Skies", "Paper Boats"]);
        assert_eq!(titles("paper"), ["Paper Boats"]);
    }
}
