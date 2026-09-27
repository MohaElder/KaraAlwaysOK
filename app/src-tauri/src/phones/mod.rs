//! Phone mics: the session guests join, the Mac's commands for it, and what phones and the Mac window hear.

mod cert;
mod room;
mod server;

use crate::player::PlayerSnapshot;
use crate::state::{AppError, AppState, Plain};
use anyhow::Context;
use axum_server::tls_rustls::RustlsConfig;
use kara_core::problem::Problem;
use qrcode::render::svg;
use qrcode::QrCode;
use room::{Out, Room, ENDED};
use serde::Serialize;
use serde_json::Value;
use std::net::IpAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::Duration;
use tauri::{AppHandle, Emitter, Manager};
use tokio::sync::mpsc::UnboundedSender;

static NEXT: AtomicU64 = AtomicU64::new(1);

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
}

#[derive(Clone, Serialize)]
pub struct JoinInfo {
    qr: String,
    code: String,
    host: String,
}

#[derive(Clone, Serialize)]
pub struct PhoneRow {
    id: String,
    name: String,
    connected: bool,
}

#[derive(Clone, Default, Serialize)]
pub struct PhonesView {
    join: Option<JoinInfo>,
    phones: Vec<PhoneRow>,
}

/// What the Mac toasts about phones.
#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum News {
    Joined { name: String, mic: usize },
}

/// What the Mac tells a phone.
#[derive(Serialize)]
#[serde(tag = "t", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum ToPhone<'a> {
    Joined,
    Player { snapshot: &'a PlayerSnapshot },
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
    Ok(Session { room, window_open: false, join, server })
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

/// Tells the Mac window who is here now.
fn changed(app: &AppHandle) {
    if let Some(view) = with(app, |s| s.view()) {
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
    open(&app, room::new_code()).await.plain()
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

/// Lets a phone in or names the close code refusing it; a phone let in hears it joined and what's playing.
pub(crate) fn admit(app: &AppHandle, code: &str, id: &str, name: &str, tx: UnboundedSender<Out>) -> Result<u64, u16> {
    let conn = NEXT.fetch_add(1, Ordering::Relaxed);
    let joined = with(app, |s| s.room.admit(code, id, name, conn, tx.clone()).map(|new| new.then_some(s.room.guests.len())))
        .ok_or(ENDED)?
        .map_err(|r| r.close_code())?;
    changed(app);
    if let Some(mic) = joined {
        let _ = app.emit("phone-news", News::Joined { name: name.to_string(), mic });
    }
    let _ = tx.send(Out::Text(encode(&ToPhone::Joined)));
    if let Ok(snapshot) = crate::player::player_state(app.state()) {
        let _ = tx.send(Out::Text(encode(&ToPhone::Player { snapshot: &snapshot })));
    }
    Ok(conn)
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
}
