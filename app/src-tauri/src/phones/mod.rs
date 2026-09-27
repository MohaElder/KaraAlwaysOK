//! Phone mics: the session guests join, the Mac's commands for it, and what phones and the Mac window hear.

mod cert;
mod room;
mod server;

use crate::adding::{self, SearchOutcome};
use crate::library::{self, Lyrics};
use crate::player::{self, PlayerSnapshot};
use crate::state::{AppError, AppState, Plain};
use anyhow::Context;
use axum_server::tls_rustls::RustlsConfig;
use kara_core::ingest::link;
use kara_core::jobs::Event;
use kara_core::library::{CollectionKind, Library, SourceKind};
use kara_core::problem::Problem;
use qrcode::render::svg;
use qrcode::QrCode;
use room::{Out, Room, ENDED};
use serde::Serialize;
use serde_json::Value;
use server::FromPhone;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc::UnboundedSender;

static NEXT: AtomicU64 = AtomicU64::new(1);
const MAX_QUERY: usize = 200;

/// The phone session while there is one; `opening` keeps two opens from starting two servers.
#[derive(Default)]
pub struct Phones {
    session: Mutex<Option<Session>>,
    opening: tauri::async_runtime::Mutex<()>,
}

struct Session {
    room: Room,
    window_open: bool,
    join: JoinInfo,
    server: axum_server::Handle,
    /// The view the Mac window last heard.
    shown: Option<PhonesView>,
    clock: Clock,
    links: Vec<PendingLink>,
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
    Refused { problem: Option<Problem> },
}

impl Session {
    fn view(&self) -> PhonesView {
        let phones = self.room.guests.iter().map(|g| PhoneRow { id: g.id.clone(), name: g.name.clone(), connected: g.tx.is_some() }).collect();
        PhonesView { join: Some(self.join.clone()), phones }
    }
}

/// Starts a session with `code`: certificate, server, join details.
async fn start(app: &AppHandle, code: &str) -> anyhow::Result<Session> {
    let ips = cert::lan_ips();
    let ip = *ips.first().context(Problem::NoNetwork)?;
    let host = cert::local_name().unwrap_or_else(|| ip.to_string());
    let dir = app.try_state::<AppState>().context(Problem::LibraryOpen)?.store.root().join("phones");
    let (cert_pem, key_pem) = cert::ensure(&dir, &host, &ips, kara_core::now_ms() / 1000).context(Problem::PhonesStart)?;
    let _ = rustls::crypto::ring::default_provider().install_default();
    let tls = RustlsConfig::from_pem(cert_pem, key_pem).await.context(Problem::PhonesStart)?;
    let server = axum_server::Handle::new();
    let (port, redirect) = server::serve(app.clone(), tls, server.clone()).context(Problem::PhonesStart)?;
    let room = Room::new(code);
    let join = join_info(&host, ip, port, redirect, &room.code).context(Problem::PhonesStart)?;
    Ok(Session { room, window_open: false, join, server, shown: None, clock: Clock::new(None, 0, false), links: Vec::new() })
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
    *phones.session.lock().unwrap() = Some(session);
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
        if let Some(g) = s.room.remove(&id) {
            g.send(Out::Close(ENDED));
        }
    });
    changed(&app);
}

/// Lets a phone in or names the close code refusing it; a phone let in hears it joined, what's playing and where the song is.
pub(crate) fn admit(app: &AppHandle, code: &str, id: &str, name: &str, tx: UnboundedSender<Out>) -> Result<u64, u16> {
    let conn = NEXT.fetch_add(1, Ordering::Relaxed);
    let (joined, clock) = with(app, |s| {
        let new = s.room.admit(code, id, name, conn, tx.clone(), kara_core::now_ms())?;
        let g = &s.room.guests;
        Ok((new.then(|| (g.len(), g[g.len() - 1].name.clone())), encode(&s.clock.message())))
    })
    .ok_or(ENDED)?
    .map_err(|r: room::Refusal| r.close_code())?;
    changed(app);
    if let Some((mic, name)) = joined {
        let _ = app.emit("phone-news", News::Joined { name, mic });
    }
    let _ = tx.send(Out::Text(encode(&ToPhone::Joined)));
    if let Ok(snapshot) = player::player_state(app.state()) {
        let _ = tx.send(Out::Text(encode(&ToPhone::Player { snapshot: &snapshot })));
    }
    let _ = tx.send(Out::Text(clock));
    Ok(conn)
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
        FromPhone::Lyrics { track_id } => library::track_lyrics(app.state(), track_id).map(|lyrics| Some(encode(&ToPhone::Lyrics { track_id, lyrics: &lyrics }))),
        FromPhone::Join { .. } | FromPhone::Ping | FromPhone::Leave => Ok(None),
    };
    match reply {
        Ok(Some(text)) => tell(app, id, text),
        Ok(None) => {}
        Err(e) => tell(app, id, encode(&ToPhone::Refused { problem: e.problem })),
    }
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

/// Adds a guest's link like the Mac does: queued once it's downloaded, or right away when it needs nothing more.
fn add_link(app: &AppHandle, id: &str, name: &str, url: String, next: bool) -> Result<(), AppError> {
    let fresh = !known_link(app, &url);
    let track = adding::add_link(app.state(), url)?;
    let _ = app.emit("library", ());
    if !adding::start_adding(app.state(), track.id)? {
        return queue_for(app, track.id, next, name);
    }
    with(app, |s| s.links.push(PendingLink { track_id: track.id, guest: id.to_string(), name: name.to_string(), next, fresh }));
    Ok(())
}

/// Whether a song in the library already comes from `url`.
fn known_link(app: &AppHandle, url: &str) -> bool {
    let Some(url) = link::parse_link(url) else { return false };
    let known = app.state::<AppState>().lib.lock().unwrap().source_by_uri(SourceKind::Link, link::canonical(&url).as_str());
    matches!(known, Ok(Some(_)))
}

/// The engine finished adding a song: a guest's link is queued now, or on failure the guest hears why and a song the link made
/// leaves the library again.
pub fn link_done(app: &AppHandle, e: &Event) {
    let (Event::Added { track_id } | Event::Failed { track_id, .. }) = e else { return };
    let Some(link) = with(app, |s| s.links.iter().position(|l| l.track_id == *track_id).map(|i| s.links.remove(i))).flatten() else { return };
    let refused = match e {
        Event::Failed { problem, .. } => {
            if link.fresh {
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
    with(app, |s| {
        let text = encode(msg);
        for g in &s.room.guests {
            g.send(Out::Text(text.clone()));
        }
    });
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
    with(&app, |s| s.clock = Clock::new(key, position_ms, playing));
    broadcast(&app, &ToPhone::Clock { key, position_ms, playing });
}

/// A phone left: its row goes, and the session ends when nobody is left and the window is closed.
pub(crate) fn leave(app: &AppHandle, id: &str) {
    let idle = with(app, |s| {
        s.room.remove(id);
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
    with(app, |s| s.room.dropped(id, conn));
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
