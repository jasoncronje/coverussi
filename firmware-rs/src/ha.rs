//! The link to Home Assistant: its WebSocket API (subscribe_entities for the
//! entities the engine follows) on a thread of its own, plus REST for art and
//! the web app's entity search.

use std::io::{Read, Write};
use std::net::{TcpStream, ToSocketAddrs};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use coverussi_app::ha::{entities_template, parse_rows, subscribe_request, Entity};
use coverussi_app::ws::{self, Decoder, Message};
use esp_idf_svc::http::Method;
use esp_idf_svc::sys::{self, EspError};
use esp_idf_svc::tls::{self, EspTls};
use log::{info, warn};
use serde_json::{json, Value};

use crate::app::{self, Msg};
use crate::config::Credentials;

static CONNECTED: AtomicBool = AtomicBool::new(false);
static FOLLOW: OnceLock<Mutex<Sender<Vec<String>>>> = OnceLock::new();
const MAX_MESSAGE: usize = 1 << 20;

pub fn connected() -> bool {
    CONNECTED.load(Ordering::Relaxed)
}

/// Subscribe to these instead (the engine's list).
pub fn follow(entities: &[String]) {
    if let Some(tx) = FOLLOW.get() {
        let _ = tx.lock().unwrap().send(entities.to_vec());
    }
}

/// A TCP socket for esp-tls to run TLS over, so reads can time out.
struct Socket(TcpStream);

impl tls::Socket for Socket {
    fn handle(&self) -> i32 {
        use std::os::fd::AsRawFd;
        self.0.as_raw_fd()
    }
    fn release(&mut self) -> Result<(), EspError> {
        Ok(()) // esp-tls closes the fd itself
    }
}

enum Stream {
    Plain(TcpStream),
    Tls(Box<EspTls<Socket>>),
}

impl Stream {
    /// Ok(0) when the read timed out.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, String> {
        match self {
            Stream::Plain(s) => match s.read(buf) {
                Ok(0) => Err("closed".into()),
                Ok(n) => Ok(n),
                Err(e) if matches!(e.kind(), std::io::ErrorKind::WouldBlock | std::io::ErrorKind::TimedOut) => Ok(0),
                Err(e) => Err(e.to_string()),
            },
            Stream::Tls(t) => match t.read(buf) {
                Ok(0) => Err("closed".into()),
                Ok(n) => Ok(n),
                Err(e) if e.code() == sys::ESP_TLS_ERR_SSL_WANT_READ => Ok(0),
                Err(e) => Err(e.to_string()),
            },
        }
    }

    fn write(&mut self, data: &[u8]) -> Result<(), String> {
        match self {
            Stream::Plain(s) => s.write_all(data).map_err(|e| e.to_string()),
            Stream::Tls(t) => t.write_all(data).map_err(|e| e.to_string()),
        }
    }
}

fn random<const N: usize>() -> [u8; N] {
    let mut b = [0u8; N];
    unsafe { sys::esp_fill_random(b.as_mut_ptr().cast(), N) };
    b
}

fn base64(bytes: &[u8]) -> String {
    const A: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    for c in bytes.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        for i in 0..4 {
            out.push(if i <= c.len() { A[(n >> (18 - 6 * i) & 63) as usize] as char } else { '=' });
        }
    }
    out
}

struct Link {
    stream: Stream,
    decoder: Decoder,
}

impl Link {
    fn open(c: &Credentials) -> Result<Link, String> {
        let secure = c.ha_url.starts_with("https://");
        let rest = c.ha_url.split_once("://").map_or(c.ha_url.as_str(), |(_, r)| r);
        let authority = rest.split('/').next().unwrap_or("");
        let host = crate::net::host_of(&c.ha_url);
        let port = authority.rsplit_once(':').and_then(|(_, p)| p.parse().ok()).unwrap_or(if secure { 443 } else { 80 });
        let dial = if c.connect_ip.is_empty() { host } else { c.connect_ip.as_str() };
        let addr = (dial, port).to_socket_addrs().map_err(|e| e.to_string())?.next().ok_or("no address")?;
        let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(10)).map_err(|e| e.to_string())?;
        tcp.set_read_timeout(Some(Duration::from_millis(250))).map_err(|e| e.to_string())?;
        tcp.set_nodelay(true).ok();
        let mut stream = if secure {
            let mut t = EspTls::adopt(Socket(tcp)).map_err(|e| e.to_string())?;
            let cfg = tls::Config { common_name: Some(host), use_crt_bundle_attach: true, ..Default::default() };
            negotiate(&mut t, host, &cfg)?;
            Stream::Tls(Box::new(t))
        } else {
            Stream::Plain(tcp)
        };
        stream.write(ws::handshake(authority, "/api/websocket", &base64(&random::<16>())).as_bytes())?;
        let mut head = Vec::new();
        let started = Instant::now();
        let end = loop {
            if let Some(end) = ws::head_end(&head) {
                break end;
            }
            if started.elapsed() > Duration::from_secs(10) || head.len() > 8192 {
                return Err("no answer to the WebSocket handshake".into());
            }
            let mut buf = [0u8; 512];
            let n = stream.read(&mut buf)?;
            head.extend_from_slice(&buf[..n]);
        };
        if !ws::accepted(&head[..end]) {
            let line = String::from_utf8_lossy(&head[..end.min(60)]).lines().next().unwrap_or("").to_string();
            return Err(format!("handshake refused: {line}"));
        }
        let mut decoder = Decoder::new(MAX_MESSAGE);
        decoder.feed(&head[end..]);
        Ok(Link { stream, decoder })
    }

    fn send(&mut self, v: &Value) -> Result<(), String> {
        self.stream.write(&ws::frame(ws::TEXT, v.to_string().as_bytes(), random()))
    }

    /// The next message, or None if nothing came for a quarter second.
    fn recv(&mut self) -> Result<Option<Message>, String> {
        loop {
            if let Some(m) = self.decoder.message().map_err(|e| format!("{e:?}"))? {
                return Ok(Some(m));
            }
            let mut buf = [0u8; 2048];
            let n = self.stream.read(&mut buf)?;
            if n == 0 {
                return Ok(None);
            }
            self.decoder.feed(&buf[..n]);
        }
    }
}

/// The handshake under a read timeout: esp-tls asks again while it waits.
fn negotiate(t: &mut EspTls<Socket>, host: &str, cfg: &tls::Config) -> Result<(), String> {
    let started = Instant::now();
    loop {
        match t.negotiate(host, cfg) {
            Ok(_) => return Ok(()),
            Err(e) if e.code() == sys::ESP_TLS_ERR_SSL_WANT_READ && started.elapsed() < Duration::from_secs(15) => {}
            Err(e) => return Err(format!("TLS: {e}")),
        }
    }
}

/// One connection, until it drops.
fn session(c: &Credentials, wanted: &mut Vec<String>, follows: &Receiver<Vec<String>>) -> Result<(), String> {
    let mut link = Link::open(c)?;
    let mut next_id = 1u64;
    let mut subscription = 0u64;
    let mut authed = false;
    let mut last_heard = Instant::now();
    let mut pinged = false;
    loop {
        let mut changed = false;
        while let Ok(w) = follows.try_recv() {
            changed |= w != *wanted;
            *wanted = w;
        }
        if authed && changed {
            if subscription != 0 {
                link.send(&json!({"id": next_id, "type": "unsubscribe_events", "subscription": subscription}))?;
                next_id += 1;
                subscription = 0;
            }
            if !wanted.is_empty() {
                subscription = next_id;
                next_id += 1;
                link.send(&subscribe_request(subscription, wanted))?;
            }
        }
        let Some(m) = link.recv()? else {
            // Quiet for a while: make sure the link is still there.
            if authed && !pinged && last_heard.elapsed() > Duration::from_secs(30) {
                link.send(&json!({"id": next_id, "type": "ping"}))?;
                next_id += 1;
                pinged = true;
            } else if pinged && last_heard.elapsed() > Duration::from_secs(45) {
                return Err("no answer to a ping".into());
            }
            continue;
        };
        last_heard = Instant::now();
        pinged = false;
        let text = match m {
            Message::Text(t) => t,
            Message::Ping(p) => {
                link.stream.write(&ws::frame(ws::PONG, &p, random()))?;
                continue;
            }
            Message::Close => return Err("closed by Home Assistant".into()),
            Message::Binary(_) => continue,
        };
        let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
        match v["type"].as_str().unwrap_or("") {
            "event" if v["id"].as_u64() == Some(subscription) => {
                app::post(Msg::Ha(v["event"].clone()));
            }
            "auth_required" => link.send(&json!({"type": "auth", "access_token": c.ha_token}))?,
            "auth_ok" => {
                info!("Home Assistant connected");
                authed = true;
                CONNECTED.store(true, Ordering::Relaxed);
                subscription = 0;
                if !wanted.is_empty() {
                    subscription = next_id;
                    next_id += 1;
                    link.send(&subscribe_request(subscription, wanted))?;
                }
                app::post(Msg::HaConnected);
            }
            "auth_invalid" => {
                warn!("Home Assistant refused the token; set a new one over USB");
                std::thread::sleep(Duration::from_secs(60));
                return Err("auth_invalid".into());
            }
            "result" if v["success"] == false => {
                warn!("Home Assistant: {}", &text[..text.len().min(200)]);
            }
            _ => {}
        }
    }
}

pub fn start(c: &Credentials) {
    let (tx, rx) = mpsc::channel();
    let _ = FOLLOW.set(Mutex::new(tx));
    if !c.home_assistant() {
        warn!("no Home Assistant set up; no sources to follow");
        return;
    }
    let c = c.clone();
    crate::spawn_in_psram("ha", 12 * 1024, move || {
        let mut wanted = Vec::new();
        let mut wait = 1;
        loop {
            if crate::net::up() {
                let started = Instant::now();
                if let Err(e) = session(&c, &mut wanted, &rx) {
                    if CONNECTED.swap(false, Ordering::Relaxed) {
                        warn!("Home Assistant disconnected: {e}");
                    } else {
                        warn!("Home Assistant: {e}");
                    }
                }
                if started.elapsed() > Duration::from_secs(60) {
                    wait = 1;
                }
            }
            std::thread::sleep(Duration::from_secs(wait));
            wait = (wait * 2).min(30);
        }
    });
}

/// A path on Home Assistant (with the token) or a URL.
pub fn fetch(c: &Credentials, key: &str, limit: usize) -> Result<crate::http::Response, String> {
    if key.starts_with('/') {
        if !c.home_assistant() {
            return Err("no Home Assistant".into());
        }
        let auth = format!("Bearer {}", c.ha_token);
        crate::http::get(&format!("{}{key}", c.ha_url), &[("Authorization", &auth)], limit)
    } else {
        crate::http::get(key, &[], limit)
    }
}

/// Entities for the web app's autocomplete. Home Assistant renders a template
/// that filters them, so the answer is a few KB, not every state.
pub fn entities(c: &Credentials, q: &str, domains: &[&str]) -> Result<Vec<Entity>, String> {
    if !c.home_assistant() {
        return Err("no Home Assistant".into());
    }
    let auth = format!("Bearer {}", c.ha_token);
    let headers = [("Authorization", auth.as_str()), ("Content-Type", "application/json")];
    let body = entities_template(q, domains);
    let url = format!("{}/api/template", c.ha_url);
    let r = crate::http::request(Method::Post, &url, &headers, body.as_bytes(), 256 << 10, Duration::from_secs(10))?;
    if r.status != 200 {
        return Err(format!("HTTP {}", r.status));
    }
    parse_rows(&r.body).ok_or_else(|| "not a list of entities".into())
}
