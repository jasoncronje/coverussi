//! The web app's live feed (/api/live): telemetry, panel frames, mic levels,
//! recognitions, events and log lines, to every open page. Sends go through a
//! thread of their own: a WebSocket send waits for the HTTP server's task, so
//! the server's own handlers (or anything logging from it) must never send directly.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender, TrySendError};
use std::sync::{Mutex, OnceLock};

use esp_idf_svc::http::server::ws::EspHttpWsDetachedSender;
use esp_idf_svc::log::{EspIdfLogFilter, EspIdfLogger};
use esp_idf_svc::ws::FrameType;
use log::{Level, LevelFilter, Log, Metadata, Record};
use serde_json::{json, Value};

enum Out {
    Text(String),
    Binary(Vec<u8>),
}

static QUEUE: OnceLock<SyncSender<Out>> = OnceLock::new();
static CLIENTS: Mutex<Vec<EspHttpWsDetachedSender>> = Mutex::new(Vec::new());
/// How many pages there are, counted apart from CLIENTS, which is empty while `run` sends.
static WATCHERS: AtomicUsize = AtomicUsize::new(0);

/// Pages watching. Telemetry and frames are only worth making for them.
pub fn watching() -> bool {
    WATCHERS.load(Ordering::Relaxed) > 0
}

pub fn add(client: EspHttpWsDetachedSender) {
    let mut clients = CLIENTS.lock().unwrap();
    clients.push(client);
    WATCHERS.fetch_add(1, Ordering::Relaxed);
}

pub fn send(v: &Value) {
    queue(Out::Text(v.to_string()));
}

/// The panel as RGB565, little-endian.
pub fn frame(f: &[u16]) {
    queue(Out::Binary(f.iter().flat_map(|p| p.to_le_bytes()).collect()));
}

fn queue(o: Out) {
    if let Some(q) = QUEUE.get() {
        if watching() {
            // A slow page loses messages; nothing here ever waits for one.
            if let Err(TrySendError::Full(_)) = q.try_send(o) {}
        }
    }
}

fn run(rx: Receiver<Out>) {
    for o in rx {
        let (kind, data) = match &o {
            Out::Text(t) => (FrameType::Text(false), t.as_bytes()),
            Out::Binary(b) => (FrameType::Binary(false), b.as_slice()),
        };
        let mut clients = std::mem::take(&mut *CLIENTS.lock().unwrap());
        clients.retain_mut(|c| !c.is_closed() && c.send(kind, data).is_ok());
        let mut shared = CLIENTS.lock().unwrap();
        clients.append(&mut shared); // ones that joined meanwhile
        WATCHERS.store(clients.len(), Ordering::Relaxed);
        *shared = clients;
    }
}

struct Logger(EspIdfLogger<EspIdfLogFilter>);

static LOGGER: Logger = Logger(EspIdfLogger::new(EspIdfLogFilter::new()));

impl Log for Logger {
    fn enabled(&self, m: &Metadata) -> bool {
        self.0.enabled(m)
    }

    fn log(&self, r: &Record) {
        self.0.log(r);
        if r.level() <= Level::Info && self.enabled(r.metadata()) {
            send(&json!({"type": "log", "line": format!("{} {}: {}", &r.level().as_str()[..1], r.target(), r.args())}));
        }
    }

    fn flush(&self) {}
}

/// Logs to USB as usual and to the web app's log.
pub fn start() {
    let (tx, rx) = mpsc::sync_channel(32);
    let _ = QUEUE.set(tx);
    crate::spawn_in_psram("live", 6144, move || run(rx));
    let _ = log::set_logger(&LOGGER);
    log::set_max_level(LevelFilter::Info);
}
