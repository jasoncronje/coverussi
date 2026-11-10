//! Updates from GitHub: what the latest release is, for the web app, and its
//! coverussi.bin downloaded straight into the other app slot. The download runs
//! on a thread of its own, its stack in internal RAM as flash writes need.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;
use std::time::Duration;

use coverussi_app::idle::Update;
use coverussi_app::version;
use esp_idf_svc::http::client::{Configuration, EspHttpConnection};
use esp_idf_svc::http::Method;
use esp_idf_svc::ota::EspOta;
use esp_idf_svc::sys;
use log::{info, warn};
use serde_json::{json, Value};

use crate::{http, live};

const REPO: &str = "jasoncronje/coverussi";
const ASSET: &str = "coverussi.bin";
/// GitHub's API refuses requests without a User-Agent.
const HEADERS: [(&str, &str); 2] = [("User-Agent", "coverussi"), ("Accept", "application/vnd.github+json")];
/// How long a failure stays on the panel.
const FAILED_SHOWN: Duration = Duration::from_secs(8);
/// The update thread's stack: an update used 6.5 KB of it (the TLS handshakes, the flash writes).
const STACK: usize = 12 * 1024;

pub struct Release {
    /// Without the tag's "v".
    pub version: String,
    pub url: String,
    pub size: usize,
    /// The release's page, with its notes.
    pub page: String,
}

/// Where an update is, and why it failed.
static STATUS: Mutex<(Option<Update>, String)> = Mutex::new((None, String::new()));
static RUNNING: AtomicBool = AtomicBool::new(false);

/// What the panel shows over everything else while it's Some.
pub fn status() -> Option<Update> {
    STATUS.lock().unwrap().0
}

/// The status for the web app.
pub fn describe() -> Value {
    let s = STATUS.lock().unwrap();
    match s.0 {
        None => Value::Null,
        Some(Update::Starting) => json!({"phase": "starting"}),
        Some(Update::Downloading(p)) => json!({"phase": "downloading", "percent": p}),
        Some(Update::Restarting) => json!({"phase": "restarting"}),
        Some(Update::Failed) => json!({"phase": "failed", "error": s.1}),
    }
}

fn set(stage: Option<Update>, error: &str) {
    *STATUS.lock().unwrap() = (stage, error.to_string());
    live::send(&json!({"type": "update", "data": describe()}));
}

/// This firmware's version, as the release tags have it.
pub fn current() -> String {
    let d = unsafe { &*sys::esp_app_get_description() };
    unsafe { core::ffi::CStr::from_ptr(d.version.as_ptr()) }.to_string_lossy().into_owned()
}

pub fn latest() -> Result<Release, String> {
    let r = http::get(&format!("https://api.github.com/repos/{REPO}/releases/latest"), &HEADERS, 256 << 10)?;
    match r.status {
        200 => {}
        404 => return Err("there's no release on GitHub yet".into()),
        s => return Err(format!("GitHub: HTTP {s}")),
    }
    let v: Value = serde_json::from_slice(&r.body).map_err(|e| format!("GitHub: {e}"))?;
    let asset = v["assets"]
        .as_array()
        .and_then(|a| a.iter().find(|a| a["name"] == ASSET))
        .ok_or_else(|| format!("the latest release has no {ASSET}"))?;
    Ok(Release {
        version: v["tag_name"].as_str().unwrap_or("").trim_start_matches('v').to_string(),
        url: asset["browser_download_url"].as_str().unwrap_or("").to_string(),
        size: asset["size"].as_u64().unwrap_or(0) as usize,
        page: v["html_url"].as_str().unwrap_or("").to_string(),
    })
}

/// Starts the update's thread; Err if one is running or there's no memory for its stack.
pub fn start() -> Result<(), String> {
    if RUNNING.swap(true, Ordering::SeqCst) {
        return Err("an update is already running".into());
    }
    set(Some(Update::Starting), "");
    if std::thread::Builder::new().name("update".into()).stack_size(STACK).spawn(run).is_err() {
        RUNNING.store(false, Ordering::SeqCst);
        set(None, "");
        return Err("not enough memory to start the update; restart the display and try again".into());
    }
    Ok(())
}

/// Download, check, restart into it.
fn run() {
    match install() {
        Ok(()) => {
            set(Some(Update::Restarting), "");
            std::thread::sleep(Duration::from_secs(2)); // RESTARTING on the panel, and the web app told
            unsafe { sys::esp_restart() };
        }
        Err(e) => {
            warn!("update failed: {e}");
            set(Some(Update::Failed), &e);
            std::thread::sleep(FAILED_SHOWN);
            set(None, "");
            RUNNING.store(false, Ordering::SeqCst);
        }
    }
}

fn install() -> Result<(), String> {
    let release = latest()?;
    let current = current();
    if !version::newer(&release.version, &current) {
        return Err(format!("{current} is already the latest"));
    }
    info!("updating {current} to {} from {}", release.version, release.url);
    // The download redirects to a signed URL of about 1 KB: room for it both ways.
    let mut c = EspHttpConnection::new(&Configuration {
        timeout: Some(Duration::from_secs(30)),
        buffer_size: Some(4096),
        buffer_size_tx: Some(4096),
        crt_bundle_attach: Some(sys::esp_crt_bundle_attach),
        ..Default::default()
    })
    .map_err(|e| e.to_string())?;
    c.initiate_request(Method::Get, &release.url, &HEADERS[..1]).map_err(|e| format!("download: {e}"))?;
    c.initiate_response().map_err(|e| format!("download: {e}"))?;
    if c.status() != 200 {
        return Err(format!("download: HTTP {}", c.status()));
    }
    let size = c.header("Content-Length").and_then(|n| n.parse().ok()).unwrap_or(release.size);
    let mut ota = EspOta::new().map_err(|e| e.to_string())?;
    let mut update = ota.initiate_update().map_err(|e| e.to_string())?;
    let mut buf = crate::heap::FlashBuffer::new(4096);
    let (mut total, mut shown) = (0usize, u8::MAX);
    loop {
        let n = match c.read(&mut buf) {
            Ok(n) => n,
            Err(e) => {
                let _ = update.abort();
                return Err(format!("download stopped: {e}"));
            }
        };
        if n == 0 {
            break;
        }
        if total == 0 && buf[0] != 0xE9 {
            let _ = update.abort();
            return Err(format!("{ASSET} isn't an ESP32 app image"));
        }
        if let Err(e) = update.write(&buf[..n]) {
            let _ = update.abort();
            return Err(format!("write failed: {e}"));
        }
        total += n;
        let percent = if size > 0 { (total * 100 / size).min(100) as u8 } else { 0 };
        if percent != shown {
            shown = percent;
            set(Some(Update::Downloading(percent)), "");
        }
    }
    if size > 0 && total != size {
        let _ = update.abort();
        return Err(format!("download stopped at {total} of {size} bytes"));
    }
    update.complete().map_err(|e| format!("not a valid image: {e}"))?;
    info!("update to {} verified ({total} bytes); restarting", release.version);
    Ok(())
}
