//! The web app: its page (built from portal/, gzipped into the firmware), its
//! API and the live feed. Open until someone sets a password in Settings.

use std::ffi::CString;

use coverussi_app::album_lock::Correction;
use coverussi_app::auth;
use coverussi_app::keys::album_key;
use coverussi_app::settings::Settings;
use esp_idf_svc::http::server::{Configuration, EspHttpConnection, EspHttpServer, Request};
use esp_idf_svc::handle::RawHandle;
use esp_idf_svc::http::Method;
use esp_idf_svc::io::Write;
use esp_idf_svc::ota::EspOta;
use esp_idf_svc::sys;
use log::info;
use serde_json::{json, Value};

use crate::app::{self, Msg};
use crate::{ha, live, net, work};

include!(concat!(env!("OUT_DIR"), "/assets.rs"));

type Req<'a, 'b> = Request<&'a mut EspHttpConnection<'b>>;
type Result = std::result::Result<(), anyhow::Error>;

const MAX_PASSWORD: usize = 128;
const COMMANDS: [&str; 7] = ["identify", "panel", "test_pattern", "panel_calibration", "restart", "calibrate", "apps"];
const MAX_BODY: usize = 16 * 1024;

/// Answers, then closes the connection. With only 7 sockets, and each open page
/// holding one for its live feed, httpd dropped the oldest idle kept-alive one
/// just as the browser reused it, and a request failed with a reset.
fn send(mut req: Req, status: u16, headers: &[(&str, &str)], bytes: &[u8]) -> Result {
    let raw = req.connection().raw_connection()?.handle();
    let (sd, fd) = unsafe { ((*raw).handle, sys::httpd_req_to_sockfd(raw)) };
    let mut headers = headers.to_vec();
    headers.push(("Connection", "close"));
    req.into_response(status, None, &headers)?.write_all(bytes)?;
    unsafe { sys::httpd_sess_trigger_close(sd, fd) };
    Ok(())
}

fn reply(req: Req, status: u16, body: &Value, cookie: Option<&str>) -> Result {
    let text = body.to_string();
    let mut headers = vec![("Content-Type", "application/json"), ("Cache-Control", "no-store")];
    if let Some(c) = cookie {
        headers.push(("Set-Cookie", c));
    }
    send(req, status, &headers, text.as_bytes())
}

fn ok(req: Req, body: &Value) -> Result {
    reply(req, 200, body, None)
}

fn error(req: Req, status: u16, detail: &str) -> Result {
    reply(req, status, &json!({"detail": detail}), None)
}

fn body(req: &mut Req, limit: usize) -> Option<Vec<u8>> {
    let mut out = Vec::new();
    let mut buf = [0u8; 1024];
    loop {
        let n = req.read(&mut buf).ok()?;
        if n == 0 {
            return Some(out);
        }
        if out.len() + n > limit {
            return None;
        }
        out.extend_from_slice(&buf[..n]);
    }
}

fn json_body(req: &mut Req) -> Option<Value> {
    serde_json::from_slice(&body(req, MAX_BODY)?).ok()
}

/// Without a password everyone is in.
fn logged_in(cookie: Option<&str>) -> bool {
    let Some(stored) = password() else { return true };
    let a = app::shared();
    cookie.and_then(auth::cookie).is_some_and(|s| auth::valid_session(&session_key(&stored), &a.boot, s, net::unix() as u64))
}

fn password() -> Option<String> {
    app::shared().store.get("web_password").filter(|p| !p.is_empty())
}

/// Sessions are signed with the password's hash too, so a new password logs everyone else out.
fn session_key(stored: &str) -> Vec<u8> {
    [&app::shared().secret[..], stored.as_bytes()].concat()
}

fn new_session(stored: &str) -> String {
    session_cookie(&auth::new_session(&session_key(stored), &app::shared().boot, net::unix() as u64), auth::SESSION_S)
}

/// The value of `key` in the URI's query, decoded.
fn query(uri: &str, key: &str) -> Option<String> {
    let q = uri.split_once('?')?.1;
    q.split('&').find_map(|kv| {
        let (k, v) = kv.split_once('=').unwrap_or((kv, ""));
        (k == key).then(|| decode(v))
    })
}

fn decode(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'+' => out.push(b' '),
            b'%' if i + 2 < b.len() => {
                match u8::from_str_radix(s.get(i + 1..i + 3).unwrap_or(""), 16) {
                    Ok(x) => {
                        out.push(x);
                        i += 2;
                    }
                    Err(_) => out.push(b'%'),
                }
            }
            c => out.push(c),
        }
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Register a handler that needs a login.
fn guarded<F>(server: &mut EspHttpServer<'static>, uri: &str, method: Method, f: F) -> anyhow::Result<()>
where
    F: for<'a, 'b> Fn(Req<'a, 'b>) -> Result + Send + 'static,
{
    server.fn_handler(uri, method, move |req: Req| -> Result {
        if !logged_in(req.header("Cookie")) {
            return error(req, 401, "log in first");
        }
        f(req)
    })?;
    Ok(())
}

fn session_cookie(value: &str, max_age: u64) -> String {
    format!("{}={value}; Path=/; Max-Age={max_age}; HttpOnly; SameSite=Lax", auth::COOKIE)
}

fn snapshot() -> Value {
    let s = app::state();
    json!({
        "type": "snapshot",
        "settings_version": s.version,
        "telemetry": s.telemetry,
        "now_playing": s.now_playing,
        "recent": s.recent,
        "rate_limit_wait_ms": work::rate_limit_wait_ms(),
        // Streamed covers are paths on Home Assistant; the Apple TV app needs its address.
        "ha_url": app::shared().creds.ha_url,
    })
}

fn set_correction(v: &Value) -> Option<String> {
    let text = |k: &str| v[k].as_str().map(str::trim).filter(|s| !s.is_empty()).map(String::from);
    let (artist, shazam_album, album) = (text("artist")?, text("shazam_album")?, text("album")?);
    let key = album_key(&artist, Some(&shazam_album));
    app::state().corrections.insert(key.clone(), Correction { album, art_url: text("art_url") });
    app::corrections_changed();
    Some(key)
}

fn install(mut req: Req) -> Result {
    let mut ota = EspOta::new()?;
    let mut update = ota.initiate_update()?;
    let mut buf = vec![0u8; 4096];
    let mut total = 0usize;
    loop {
        let n = match req.read(&mut buf) {
            Ok(n) => n,
            Err(e) => {
                update.abort()?;
                return error(req, 400, &format!("upload failed: {e:?}"));
            }
        };
        if n == 0 {
            break;
        }
        if total == 0 && buf[0] != 0xE9 {
            update.abort()?;
            return error(req, 400, "not an ESP32 app image (a .bin from the release)");
        }
        if let Err(e) = update.write(&buf[..n]) {
            update.abort()?;
            return error(req, 400, &format!("write failed: {e}"));
        }
        total += n;
    }
    if let Err(e) = update.complete() {
        return error(req, 400, &format!("not a valid image: {e}"));
    }
    info!("firmware update of {total} bytes verified; restarting");
    ok(req, &json!({"ok": true, "size": total}))?;
    // Restarts from here: a thread to do it needs an 8 KB stack, more internal
    // RAM than is free, and failing to start one crashed the board.
    std::thread::sleep(std::time::Duration::from_secs(1));
    unsafe { sys::esp_restart() };
    Ok(())
}

fn cookie_of(raw: *mut sys::httpd_req_t) -> Option<String> {
    let name = CString::new("Cookie").ok()?;
    unsafe {
        let n = sys::httpd_req_get_hdr_value_len(raw, name.as_ptr());
        if n == 0 {
            return None;
        }
        let mut buf = vec![0u8; n + 1];
        if sys::httpd_req_get_hdr_value_str(raw, name.as_ptr(), buf.as_mut_ptr().cast(), buf.len()) != sys::ESP_OK {
            return None;
        }
        buf.truncate(n);
        String::from_utf8(buf).ok()
    }
}

pub fn start() -> anyhow::Result<EspHttpServer<'static>> {
    let mut server = EspHttpServer::new(&Configuration {
        stack_size: 12 * 1024,
        max_uri_handlers: 28,
        max_open_sockets: 7,
        uri_match_wildcard: true,
        lru_purge_enable: true,
        ..Default::default()
    })?;

    for (path, kind, bytes) in ASSETS {
        let uri = if *path == "/index.html" { "/" } else { path };
        server.fn_handler(uri, Method::Get, move |req: Req| -> Result {
            let cache = if uri == "/" { "no-cache" } else { "public, max-age=31536000, immutable" };
            send(req, 200, &[("Content-Type", *kind), ("Content-Encoding", "gzip"), ("Cache-Control", cache)], bytes)
        })?;
    }

    server.fn_handler("/api/session", Method::Get, |req: Req| -> Result {
        let logged = logged_in(req.header("Cookie"));
        ok(req, &json!({"logged_in": logged, "password_set": password().is_some()}))
    })?;

    server.fn_handler("/api/login", Method::Post, |mut req: Req| -> Result {
        let given = json_body(&mut req).and_then(|v| v["password"].as_str().map(String::from)).unwrap_or_default();
        let Some(stored) = password() else { return ok(req, &json!({"ok": true})) };
        if !auth::password_ok(&stored, &given) {
            return error(req, 401, "wrong password");
        }
        reply(req, 200, &json!({"ok": true}), Some(&new_session(&stored)))
    })?;

    // {"password": "..."} sets or changes it and keeps this browser logged in; "" removes it.
    guarded(&mut server, "/api/password", Method::Post, |mut req| {
        let given = json_body(&mut req).and_then(|v| v["password"].as_str().map(String::from)).unwrap_or_default();
        let store = &app::shared().store;
        if given.is_empty() {
            store.set("web_password", b"");
            info!("web app password removed");
            return reply(req, 200, &json!({"ok": true}), Some(&session_cookie("", 0)));
        }
        let n = given.chars().count();
        if !(auth::MIN_PASSWORD..=MAX_PASSWORD).contains(&n) {
            return error(req, 400, &format!("use {} to {MAX_PASSWORD} characters", auth::MIN_PASSWORD));
        }
        let mut salt = [0u8; 16];
        unsafe { sys::esp_fill_random(salt.as_mut_ptr().cast(), 16) };
        let stored = auth::hash_password(&given, &salt);
        if !store.set("web_password", stored.as_bytes()) {
            return error(req, 500, "couldn't save the password");
        }
        info!("web app password set");
        reply(req, 200, &json!({"ok": true}), Some(&new_session(&stored)))
    })?;

    guarded(&mut server, "/api/time_zones", Method::Get, |req| {
        let names: Vec<&str> = coverussi_app::tz::names().collect();
        ok(req, &json!(names))
    })?;

    server.fn_handler("/api/logout", Method::Post, |req: Req| -> Result {
        reply(req, 200, &json!({"ok": true}), Some(&session_cookie("", 0)))
    })?;

    guarded(&mut server, "/api/state", Method::Get, |req| ok(req, &snapshot()))?;

    guarded(&mut server, "/api/settings", Method::Get, |req| {
        let s = app::state();
        let body = json!({"version": s.version, "settings": s.settings, "defaults": Settings::default()});
        drop(s);
        ok(req, &body)
    })?;

    guarded(&mut server, "/api/settings", Method::Put, |mut req| {
        let Some(raw) = body(&mut req, MAX_BODY) else { return error(req, 413, "too big") };
        match Settings::from_json(&String::from_utf8_lossy(&raw)) {
            Ok(s) => {
                let v = app::save_settings(s);
                ok(req, &json!({"version": v}))
            }
            Err(e) => error(req, 422, &e.to_string()),
        }
    })?;

    guarded(&mut server, "/api/commands", Method::Post, |mut req| {
        let v = json_body(&mut req).unwrap_or_default();
        let name = v["name"].as_str().unwrap_or("");
        if !COMMANDS.contains(&name) {
            return error(req, 422, &format!("unknown command {name:?}"));
        }
        let sent = app::post(Msg::Command(name.to_string(), v["args"].clone()));
        ok(req, &json!({"ok": sent}))
    })?;

    guarded(&mut server, "/api/pin", Method::Post, |req| ok(req, &json!({"ok": work::pin()})))?;

    guarded(&mut server, "/api/corrections", Method::Get, |req| {
        let c = serde_json::to_value(&app::state().corrections)?;
        ok(req, &c)
    })?;

    guarded(&mut server, "/api/corrections", Method::Post, |mut req| {
        match json_body(&mut req).as_ref().and_then(set_correction) {
            Some(key) => ok(req, &json!({"key": key})),
            None => error(req, 422, "artist, shazam_album and album are needed"),
        }
    })?;

    guarded(&mut server, "/api/corrections/*", Method::Delete, |req| {
        let key = decode(req.uri().trim_start_matches("/api/corrections/"));
        let gone = app::state().corrections.remove(&key).is_some();
        if gone {
            app::corrections_changed();
        }
        ok(req, &json!({"ok": gone}))
    })?;

    guarded(&mut server, "/api/ha/entities*", Method::Get, |req| {
        let c = &app::shared().creds;
        if !c.home_assistant() {
            return ok(req, &json!({"available": false, "entities": []}));
        }
        let uri = req.uri().to_string();
        let q = query(&uri, "q").unwrap_or_default();
        let domain = query(&uri, "domain").unwrap_or_default();
        let domains: Vec<&str> = domain.split(',').map(str::trim).filter(|d| !d.is_empty()).collect();
        let limit = query(&uri, "limit").and_then(|l| l.parse().ok()).unwrap_or(20usize).clamp(1, 50);
        match ha::entities(c, &q, &domains) {
            Ok(all) => {
                let found = coverussi_app::ha::search(&all, &q, &domains, limit);
                ok(req, &json!({"available": true, "entities": found}))
            }
            Err(e) => error(req, 502, &format!("Home Assistant: {e}")),
        }
    })?;

    // An entity's state and text attributes with their possible values, for an analog source's trigger.
    guarded(&mut server, "/api/ha/triggers*", Method::Get, |req| {
        let c = &app::shared().creds;
        let entity = query(req.uri(), "entity").unwrap_or_default();
        if !coverussi_app::ha::valid_entity(&entity) {
            return error(req, 400, "Not an entity id");
        }
        if !c.home_assistant() {
            return error(req, 409, "Home Assistant isn't set up");
        }
        match ha::fetch(c, &format!("/api/states/{entity}"), 64 << 10) {
            Ok(r) if r.status == 404 => error(req, 404, &format!("Home Assistant has no {entity}")),
            Ok(r) => match coverussi_app::ha::triggers_from_state(&r.body) {
                Some(t) if r.status == 200 => ok(req, &t),
                _ => error(req, 502, &format!("Home Assistant: HTTP {}", r.status)),
            },
            Err(e) => error(req, 502, &format!("Home Assistant: {e}")),
        }
    })?;

    // A player's apps straight from Home Assistant, so a player added but not saved can list them.
    guarded(&mut server, "/api/ha/apps*", Method::Get, |req| {
        let c = &app::shared().creds;
        let entity = query(req.uri(), "entity").unwrap_or_default();
        if !coverussi_app::ha::valid_entity(&entity) {
            return error(req, 400, "Not an entity id");
        }
        if !c.home_assistant() {
            return error(req, 409, "Home Assistant isn't set up");
        }
        match ha::fetch(c, &format!("/api/states/{entity}"), 64 << 10) {
            Ok(r) if r.status == 404 => error(req, 404, &format!("Home Assistant has no {entity}")),
            Ok(r) => match coverussi_app::ha::apps_from_state(&entity, &r.body) {
                Some(apps) if r.status == 200 => ok(req, &apps),
                _ => error(req, 502, &format!("Home Assistant: HTTP {}", r.status)),
            },
            Err(e) => error(req, 502, &format!("Home Assistant: {e}")),
        }
    })?;

    guarded(&mut server, "/api/firmware", Method::Post, install)?;

    server.ws_handler("/api/live", None, |ws| -> std::result::Result<(), sys::EspError> {
        use esp_idf_svc::http::server::ws::EspHttpWsConnection;
        if let EspHttpWsConnection::New(sd, raw) = ws {
            if !logged_in(cookie_of(*raw).as_deref()) {
                let (sd, fd) = (*sd, unsafe { sys::httpd_req_to_sockfd(*raw) });
                let _ = ws.send(esp_idf_svc::ws::FrameType::Close, &4401u16.to_be_bytes());
                unsafe { sys::httpd_sess_trigger_close(sd, fd) };
                return Ok(());
            }
            live::add(ws.create_detached_sender()?);
            live::send(&snapshot());
            let frame = app::state().frame.clone();
            if !frame.is_empty() {
                live::frame(&frame);
            }
            app::post(Msg::Watcher);
            return Ok(());
        }
        if let EspHttpWsConnection::Receiving(sd, raw, _) = ws {
            // Pages don't send anything. A failed read is a page that went away
            // without closing: close it, or httpd calls again for it forever.
            let (sd, fd) = (*sd, unsafe { sys::httpd_req_to_sockfd(*raw) });
            let mut buf = [0u8; 64];
            if ws.recv(&mut buf).is_err() {
                unsafe { sys::httpd_sess_trigger_close(sd, fd) };
            }
        }
        Ok(())
    })?;

    info!("web app on http://{}/", net::ip());
    Ok(server)
}
