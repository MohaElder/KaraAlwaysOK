//! The HTTPS server phones use: the phone page, song pictures, and one WebSocket per phone.

use super::room::Out;
use anyhow::Context;
use axum::extract::ws::{CloseFrame, Message, Utf8Bytes, WebSocket, WebSocketUpgrade};
use axum::extract::{ConnectInfo, Path, Request, State};
use axum::http::{header, HeaderMap, Method, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Redirect, Response};
use axum::routing::get;
use axum::Router;
use axum_server::tls_rustls::RustlsConfig;
use serde::Deserialize;
use std::net::{SocketAddr, TcpListener};
use std::time::Duration;
use tauri::{AppHandle, Manager};
use tokio::sync::mpsc;
use tokio::time::Instant;

const SILENT: Duration = Duration::from_secs(5);

/// What a phone tells the Mac.
#[derive(Deserialize)]
#[serde(tag = "t", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub(crate) enum FromPhone {
    Join { code: String, id: String, name: String },
    Ping,
    Leave,
}

/// Serves on port 443 (8443 when taken) and sends plain requests on port 80 there; returns the HTTPS port and whether port 80 is
/// served. A debug build with `KARA_PHONE_PORT` serves only that port and leaves port 80 alone.
pub fn serve(app: AppHandle, tls: RustlsConfig, handle: axum_server::Handle) -> anyhow::Result<(u16, bool)> {
    let chosen = if cfg!(debug_assertions) { std::env::var("KARA_PHONE_PORT").ok().and_then(|p| p.parse::<u16>().ok()) } else { None };
    let ports = chosen.map_or(vec![443, 8443], |p| vec![p]);
    let (listener, port) = ports.into_iter().find_map(|p| TcpListener::bind(("0.0.0.0", p)).ok().map(|l| (l, p))).context("no free port")?;
    listener.set_nonblocking(true)?;
    let routes = Router::new()
        .route("/ws", get(socket))
        .route("/art/{name}", get(art))
        .route("/", get(|| async { Redirect::temporary("/phone") }))
        .fallback(page)
        .layer(middleware::from_fn(lan_only))
        .with_state(app);
    let secure = axum_server::from_tcp_rustls(listener, tls).handle(handle.clone());
    tauri::async_runtime::spawn(secure.serve(routes.into_make_service_with_connect_info::<SocketAddr>()));
    if chosen.is_some() {
        return Ok((port, false));
    }
    let Ok(plain) = TcpListener::bind(("0.0.0.0", 80)) else { return Ok((port, false)) };
    plain.set_nonblocking(true)?;
    let to_https = move |headers: HeaderMap| async move {
        let host = headers.get(header::HOST).and_then(|h| h.to_str().ok()).unwrap_or_default();
        let host = host.split(':').next().unwrap_or_default();
        let port = if port == 443 { String::new() } else { format!(":{port}") };
        Redirect::temporary(&format!("https://{host}{port}/phone"))
    };
    tauri::async_runtime::spawn(axum_server::from_tcp(plain).handle(handle).serve(Router::new().fallback(to_https).into_make_service()));
    Ok((port, true))
}

async fn lan_only(ConnectInfo(peer): ConnectInfo<SocketAddr>, request: Request, next: Next) -> Response {
    if super::cert::is_lan(peer.ip()) {
        next.run(request).await
    } else {
        StatusCode::FORBIDDEN.into_response()
    }
}

/// The phone page and its files: from the dev server under `tauri dev`, from the app's own files otherwise. Only GET.
async fn page(State(app): State<AppHandle>, method: Method, uri: Uri) -> Response {
    if method != Method::GET {
        return StatusCode::METHOD_NOT_ALLOWED.into_response();
    }
    if tauri::is_dev() {
        let Some(base) = app.config().build.dev_url.clone() else { return StatusCode::NOT_FOUND.into_response() };
        let Ok(url) = base.join(uri.path_and_query().map_or("/", |p| p.as_str())) else { return StatusCode::BAD_REQUEST.into_response() };
        let Ok(r) = reqwest::get(url).await else { return StatusCode::BAD_GATEWAY.into_response() };
        let status = StatusCode::from_u16(r.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
        let kind = r.headers().get(header::CONTENT_TYPE).cloned();
        let mut res = (status, r.bytes().await.unwrap_or_default()).into_response();
        if let Some(kind) = kind {
            res.headers_mut().insert(header::CONTENT_TYPE, kind);
        }
        return res;
    }
    match app.asset_resolver().get(uri.path().to_string()) {
        Some(a) => ([(header::CONTENT_TYPE, a.mime_type)], a.bytes).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

/// A song picture from the artwork folder.
async fn art(State(app): State<AppHandle>, Path(name): Path<String>) -> Response {
    if name.contains(['/', '\\']) || name.starts_with('.') {
        return StatusCode::NOT_FOUND.into_response();
    }
    let kind = match name.rsplit('.').next() {
        Some("png") => "image/png",
        Some("webp") => "image/webp",
        _ => "image/jpeg",
    };
    let dir = app.state::<crate::state::AppState>().store.artwork_dir();
    match std::fs::read(dir.join(&name)) {
        Ok(bytes) => ([(header::CONTENT_TYPE, kind)], bytes).into_response(),
        Err(_) => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn socket(State(app): State<AppHandle>, upgrade: WebSocketUpgrade) -> Response {
    upgrade.on_upgrade(move |ws| phone(app, ws))
}

/// One phone's connection: it joins first, then messages flow both ways until either side closes or the phone goes silent.
async fn phone(app: AppHandle, mut ws: WebSocket) {
    let Ok(Some(Ok(Message::Text(first)))) = tokio::time::timeout(Duration::from_secs(10), ws.recv()).await else { return };
    let Ok(FromPhone::Join { code, id, name }) = serde_json::from_str(first.as_str()) else { return };
    let (tx, mut rx) = mpsc::unbounded_channel();
    let conn = match super::admit(&app, &code, &id, &name, tx) {
        Ok(conn) => conn,
        Err(close) => return close_with(&mut ws, close).await,
    };
    let mut heard = Instant::now();
    loop {
        tokio::select! {
            out = rx.recv() => match out {
                Some(Out::Text(text)) => if ws.send(Message::Text(text.into())).await.is_err() { break },
                Some(Out::Close(close)) => return close_with(&mut ws, close).await,
                None => return,
            },
            msg = ws.recv() => {
                heard = Instant::now();
                match msg {
                    Some(Ok(Message::Text(text))) => {
                        if let Ok(FromPhone::Leave) = serde_json::from_str(text.as_str()) {
                            super::leave(&app, &id);
                            return close_with(&mut ws, 1000).await;
                        }
                    }
                    Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                    Some(Ok(_)) => {}
                }
            },
            _ = tokio::time::sleep_until(heard + SILENT) => break,
        }
    }
    super::dropped(&app, &id, conn);
}

async fn close_with(ws: &mut WebSocket, code: u16) {
    let _ = ws.send(Message::Close(Some(CloseFrame { code, reason: Utf8Bytes::from_static("") }))).await;
}
