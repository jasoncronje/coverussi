//! The app task, which owns the engine. Every other thread (Home Assistant, the
//! mic, downloads, the web app) talks to it through `post`, and reads what it
//! publishes from `shared()`.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Receiver, SyncSender};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use coverussi_app::album_lock::Correction;
use coverussi_app::art::{self, Compositor, Frame};
use coverussi_app::engine::{Engine, Platform};
use coverussi_app::idle::{self, Boot};
use coverussi_app::model::Ms;
use coverussi_app::plays::{PlayFollower, Showing};
use coverussi_app::recent::Recent;
use coverussi_app::settings::{Color, Settings, Transition};
use coverussi_app::tz::posix_tz;
use esp_idf_svc::hal::gpio::{Input, PinDriver};
use esp_idf_svc::sys;
use log::{info, warn};
use serde_json::{json, Map, Value};

use crate::config::{Credentials, Store};
use crate::{audio, board, ha, live, net, update, work};

const TICK_MS: Ms = 33;
const FRAME_EVERY_MS: Ms = 200;

pub enum Msg {
    /// A subscribe_entities event.
    Ha(Value),
    HaConnected,
    /// A 50 ms mic block, dBFS.
    Level(f32),
    /// A recognition answer.
    Answer(Value),
    ClipFailed,
    Image { key: String, rgb: Vec<u8>, w: usize, h: usize },
    ImageFailed,
    Command(String, Value),
    Settings(Box<Settings>, u32),
    /// A page opened the live feed: send it the state now.
    Watcher,
}

/// What the web app reads.
pub struct State {
    pub settings: Settings,
    pub version: u32,
    pub telemetry: Value,
    /// The analog source's recognised song: title, artist, album, art, timing.
    pub now_playing: Value,
    pub recent: Recent,
    pub corrections: HashMap<String, Correction>,
    /// The panel as it is, RGB565.
    pub frame: Vec<u16>,
}

pub struct Shared {
    pub creds: Credentials,
    pub store: Store,
    pub secret: Vec<u8>,
    /// Random each start: signs sessions made before the clock is set.
    pub boot: [u8; 16],
    pub state: Mutex<State>,
}

static SHARED: OnceLock<Shared> = OnceLock::new();
static QUEUE: OnceLock<SyncSender<Msg>> = OnceLock::new();
/// The recent songs changed since they were saved.
static RECENT_UNSAVED: AtomicBool = AtomicBool::new(false);

pub fn shared() -> &'static Shared {
    SHARED.get().expect("app::init first")
}

pub fn state() -> std::sync::MutexGuard<'static, State> {
    shared().state.lock().unwrap()
}

/// Hand a message to the app task; dropped (false) if its queue is full.
pub fn post(m: Msg) -> bool {
    QUEUE.get().is_some_and(|q| q.try_send(m).is_ok())
}

pub fn now_playing(v: Value) {
    crate::mqtt::now_playing(&v);
    state().now_playing = v.clone();
    live::send(&json!({"type": "now_playing", "data": v}));
}

/// Change the recent songs; sent to the web app when they change, and saved by
/// the app task: the threads that call this have stacks in PSRAM, which can't write flash.
pub fn add_recent(f: impl FnOnce(&mut Recent) -> bool) {
    let mut s = state();
    if f(&mut s.recent) {
        let text = s.recent.to_json();
        drop(s);
        RECENT_UNSAVED.store(true, Ordering::Relaxed);
        live::send(&json!({"type": "recent", "songs": serde_json::from_str::<Value>(&text).unwrap_or_default()}));
    }
}

pub fn corrections_changed() {
    let s = state();
    let text = serde_json::to_string(&s.corrections).unwrap_or_default();
    work::apply_settings(&s.settings, &s.corrections);
    drop(s);
    shared().store.set("corrections", text.as_bytes());
}

/// Load what NVS has and start the queue; the engine runs in `run`.
pub fn init(creds: Credentials, store: Store) -> Receiver<Msg> {
    let settings = store
        .get("settings")
        .and_then(|j| Settings::from_json(&j).map_err(|e| warn!("saved settings unusable: {e}")).ok())
        .unwrap_or_default();
    let version = store.u32("settings_ver");
    let recent = Recent::from_json(&store.get("recent").unwrap_or_default());
    let corrections = store.get("corrections").and_then(|j| serde_json::from_str(&j).ok()).unwrap_or_default();
    let secret = store.secret();
    let state = State {
        settings,
        version,
        telemetry: Value::Null,
        now_playing: Value::Null,
        recent,
        corrections,
        frame: Vec::new(),
    };
    let mut boot = [0u8; 16];
    unsafe { sys::esp_fill_random(boot.as_mut_ptr().cast(), boot.len()) };
    let _ = SHARED.set(Shared { creds, store, secret, boot, state: Mutex::new(state) });
    let (tx, rx) = mpsc::sync_channel(48);
    let _ = QUEUE.set(tx);
    rx
}

/// Save settings from the web app; the app task applies them. The new version.
pub fn save_settings(s: Settings) -> u32 {
    let mut st = state();
    st.version += 1;
    let v = st.version;
    st.settings = s.clone();
    drop(st);
    let store = &shared().store;
    store.set("settings", serde_json::to_string(&s).unwrap_or_default().as_bytes());
    store.set_u32("settings_ver", v);
    post(Msg::Settings(Box::new(s), v));
    live::send(&json!({"type": "settings", "version": v}));
    v
}

fn now() -> Ms {
    (unsafe { sys::esp_timer_get_time() } / 1000) as Ms + 1 // never 0: 0 means unset in the model
}

#[derive(Default)]
struct Board {
    plays: PlayFollower,
    last_frame: Ms,
    /// The colour tuning; None while it changes nothing.
    grade: Option<art::Grade>,
    /// Clockwise degrees for how the panel is mounted.
    rotation: i64,
    /// The engine's last frame, redrawn when the tuning or rotation changes, its
    /// tuned copy and its turned copy.
    shown: Option<Box<Frame>>,
    graded: Option<Box<Frame>>,
    rotated: Option<Box<Frame>>,
    /// The start-up screen, shown instead of the engine's frames while it's up.
    boot: Option<Box<Frame>>,
    /// A frame the web app hasn't had: the last one of a transition can land
    /// between sends, and with the picture still after it nothing else would.
    unsent: bool,
}

impl Board {
    fn draw(&mut self) {
        let Some(mut f) = self.boot.as_deref().or(self.shown.as_deref()) else { return };
        if let Some(grade) = &self.grade {
            let out = self.graded.get_or_insert_with(|| Box::new([0; art::SIZE * art::SIZE]));
            grade.apply(f, out);
            f = &**out;
        }
        if self.rotation != 0 {
            let out = self.rotated.get_or_insert_with(|| Box::new([0; art::SIZE * art::SIZE]));
            art::rotate(f, self.rotation, out);
            f = &**out;
        }
        board::panel_draw(f);
    }

    /// The latest frame to the web app, at most every FRAME_EVERY_MS.
    fn send_frame(&mut self) {
        let t = now();
        if !self.unsent || t - self.last_frame < FRAME_EVERY_MS {
            return;
        }
        let Some(f) = self.shown.as_deref() else { return };
        self.last_frame = t;
        self.unsent = false;
        state().frame = f.to_vec();
        live::frame(f);
    }
}

impl Platform for Board {
    fn follow(&mut self, entities: &[String]) {
        ha::follow(entities);
    }

    fn want_art(&mut self, key: &str) {
        work::fetch_art(key);
    }

    fn mic(&mut self, on: bool) {
        audio::enable(on);
    }

    fn record_clip(&mut self, ms: u32) {
        let source = state().telemetry["source_name"].as_str().unwrap_or("").to_string();
        if net::up() {
            audio::record(ms, &source);
        } else {
            post(Msg::ClipFailed);
        }
    }

    fn frame(&mut self, f: &Frame) {
        self.shown.get_or_insert_with(|| Box::new([0; art::SIZE * art::SIZE])).copy_from_slice(f);
        self.draw();
        self.unsent = true;
        self.send_frame();
    }

    fn color(&mut self, c: &Color) {
        self.grade = art::Grade::new(c);
        self.draw();
    }

    fn send(&mut self, m: Value) {
        match m["type"].as_str() {
            Some("telemetry") => {
                let d = &m["data"];
                let showing = Showing {
                    digital: d["mode"] == "digital",
                    source: d["source"].as_str().unwrap_or(""),
                    artist: d["artist"].as_str().unwrap_or(""),
                    title: d["title"].as_str().unwrap_or(""),
                };
                if let Some(p) = self.plays.update(net::unix(), &showing) {
                    work::played(p);
                }
                crate::mqtt::panel(d["panel_on"] == true, d["brightness"].as_i64().unwrap_or(0));
                state().telemetry = d.clone();
            }
            Some("event") if m["name"] == "analog_stopped" => {
                work::end_session();
                now_playing(Value::Null);
            }
            _ => {}
        }
        live::send(&m);
    }

    fn command(&mut self, name: &str, _args: &Value) {
        match name {
            "restart" => {
                warn!("restart from the web app");
                std::thread::sleep(Duration::from_millis(500));
                unsafe { sys::esp_restart() };
            }
            _ => warn!("unknown command {name}"),
        }
    }

    fn telemetry_extras(&mut self, t: &mut Map<String, Value>) {
        let d = unsafe { &*sys::esp_app_get_description() };
        let version = unsafe { core::ffi::CStr::from_ptr(d.version.as_ptr()) }.to_string_lossy();
        t.insert("firmware".into(), version.into());
        t.insert("uptime_s".into(), (unsafe { sys::esp_timer_get_time() } / 1_000_000).into());
        t.insert("free_heap".into(), unsafe { sys::heap_caps_get_free_size(sys::MALLOC_CAP_INTERNAL) }.into());
        t.insert("min_free_heap".into(), unsafe { sys::heap_caps_get_minimum_free_size(sys::MALLOC_CAP_INTERNAL) }.into());
        t.insert("restart_reason".into(), restart_reason().into());
        t.insert("free_psram".into(), unsafe { sys::heap_caps_get_free_size(sys::MALLOC_CAP_SPIRAM) }.into());
        t.insert("wifi_rssi".into(), net::rssi().into());
        t.insert("wifi_ssid".into(), shared().creds.ssid.clone().into());
        t.insert("ip".into(), net::ip().into());
        t.insert("ha_connected".into(), ha::connected().into());
        t.insert("clock_set".into(), net::clock_set().into());
        t.insert("audio".into(), if audio::ok() { "mics" } else { "mics failed" }.into());
        t.insert("rate_limit_wait_ms".into(), work::rate_limit_wait_ms().into());
    }
}

/// Why the board last started: a crash shows up here instead of looking like a
/// restart. An update and the Restart button both read "restart".
fn restart_reason() -> &'static str {
    match unsafe { sys::esp_reset_reason() } {
        sys::esp_reset_reason_t_ESP_RST_POWERON => "power on",
        sys::esp_reset_reason_t_ESP_RST_SW => "restart",
        sys::esp_reset_reason_t_ESP_RST_PANIC => "crash",
        sys::esp_reset_reason_t_ESP_RST_INT_WDT
        | sys::esp_reset_reason_t_ESP_RST_TASK_WDT
        | sys::esp_reset_reason_t_ESP_RST_WDT => "hang",
        sys::esp_reset_reason_t_ESP_RST_BROWNOUT | sys::esp_reset_reason_t_ESP_RST_PWR_GLITCH => "power dip",
        sys::esp_reset_reason_t_ESP_RST_EXT => "reset button",
        sys::esp_reset_reason_t_ESP_RST_USB | sys::esp_reset_reason_t_ESP_RST_JTAG => "USB",
        _ => "unknown",
    }
}

fn apply(engine: &mut Engine, b: &mut Board, s: Settings, version: u32, t: Ms) {
    net::set_time_zone(posix_tz(&s.display.time_zone).unwrap_or("UTC0"));
    audio::set_gain(s.listening.mic_gain_db);
    work::apply_settings(&s, &state().corrections);
    if b.rotation != s.display.rotation {
        b.rotation = s.display.rotation;
        b.draw();
    }
    engine.settings(s, version, t, b);
}

/// The start-up screen: up until Home Assistant answers (or Wi-Fi, without
/// Home Assistant), then READY for a second. After a minute the panel goes back
/// to the engine whatever is missing; the web app says what.
#[derive(Default)]
struct Starting {
    ready_at: Option<Ms>,
    compositor: Compositor,
    frames: u64,
}

impl Starting {
    const MAX_MS: Ms = 60_000;
    const READY_MS: Ms = 1000;
    const BRIGHTNESS: u8 = 60;

    /// Draws the next frame; false once it's done.
    fn tick(&mut self, t: Ms, b: &mut Board) -> bool {
        let stage = if !net::up() {
            Boot::WiFi
        } else if !shared().creds.ha_url.is_empty() && !ha::connected() {
            Boot::HomeAssistant
        } else {
            Boot::Ready
        };
        let ready_at = *self.ready_at.get_or_insert(if stage == Boot::Ready { t } else { Ms::MAX });
        if stage != Boot::Ready {
            self.ready_at = None;
        }
        if t >= Self::MAX_MS || t.saturating_sub(ready_at) >= Self::READY_MS {
            return false;
        }
        self.frames += 1;
        self.compositor.show(Some(Arc::from(idle::boot(stage, t))), &self.frames.to_string(), t, 0, Transition::Cut);
        self.compositor.render(t, b.boot.get_or_insert_with(|| Box::new([0; art::SIZE * art::SIZE])));
        b.draw();
        true
    }
}

/// The update screen, over everything while an update from GitHub runs.
#[derive(Default)]
struct Updating {
    compositor: Compositor,
    frames: u64,
    on: bool,
}

impl Updating {
    fn tick(&mut self, t: Ms, b: &mut Board) {
        match update::status() {
            Some(stage) => {
                self.on = true;
                self.frames += 1;
                self.compositor.show(Some(Arc::from(idle::update(stage, t))), &self.frames.to_string(), t, 0, Transition::Cut);
                self.compositor.render(t, b.boot.get_or_insert_with(|| Box::new([0; art::SIZE * art::SIZE])));
                b.draw();
            }
            None if self.on => {
                self.on = false;
                b.boot = None;
                b.draw();
            }
            None => {}
        }
    }
}

/// BOOT: a press turns the panel off and on, a double press identifies what's
/// playing (an analog source, or the room), holding 2 s shows the test pattern.
#[derive(Default)]
struct Button {
    down: Ms,
    /// A press that may be the first of two.
    released: Ms,
    held: bool,
}

impl Button {
    fn poll(&mut self, pressed: bool, t: Ms, engine: &mut Engine, b: &mut Board) {
        const DOUBLE_MS: Ms = 400;
        if pressed && self.down == 0 {
            self.down = t;
            self.held = false;
        } else if pressed && !self.held && t - self.down >= 2000 {
            self.held = true;
            self.released = 0;
            engine.command("test_pattern", &Value::Null, t, b);
        } else if !pressed && self.down != 0 {
            if !self.held && t - self.down >= 30 {
                if self.released != 0 {
                    self.released = 0;
                    engine.command("identify", &Value::Null, t, b);
                } else {
                    self.released = t;
                }
            }
            self.down = 0;
        }
        if self.released != 0 && self.down == 0 && t - self.released >= DOUBLE_MS {
            self.released = 0;
            let on = !engine.device.panel_on();
            engine.command("panel", &json!({"on": on}), t, b);
        }
    }
}

pub fn run(rx: Receiver<Msg>, boot: PinDriver<'static, Input>) -> ! {
    let mut engine = Engine::new();
    let mut b = Board::default();
    let mut button = Button::default();
    let mut starting = Some(Starting::default());
    let mut updating = Updating::default();
    let mut brightness_shown = -1;
    let (settings, version) = {
        let s = state();
        (s.settings.clone(), s.version)
    };
    apply(&mut engine, &mut b, settings, version, now());
    let mut next_tick = now();
    loop {
        let t = now();
        let wait = Duration::from_millis(next_tick.saturating_sub(t));
        if let Ok(m) = rx.recv_timeout(wait) {
            let t = now();
            match m {
                Msg::Ha(event) => engine.ha_event(&event, t, &mut b),
                Msg::HaConnected => engine.refollow(),
                Msg::Level(db) => {
                    engine.level(db, t, &mut b);
                    level(db);
                }
                Msg::Answer(a) => engine.recognition(&a, t, &mut b),
                Msg::ClipFailed => engine.clip_failed(t, &mut b),
                Msg::Image { key, rgb, w, h } => engine.image(&key, &rgb, w, h, t, &mut b),
                Msg::ImageFailed => engine.image_failed(t, &mut b),
                Msg::Command(name, args) => {
                    info!("command {name}");
                    engine.command(&name, &args, t, &mut b);
                }
                Msg::Settings(s, v) => {
                    info!("settings version {v}");
                    apply(&mut engine, &mut b, *s, v, t);
                }
                Msg::Watcher => engine.announce(t, &mut b),
            }
        }
        if now() >= next_tick {
            let t = now();
            engine.set_clock(net::local_time());
            engine.tick(t, &mut b);
            b.send_frame();
            if starting.as_mut().is_some_and(|s| !s.tick(t, &mut b)) {
                starting = None;
                b.boot = None;
                b.draw();
            }
            updating.tick(t, &mut b);
            let shown = if starting.is_some() || updating.on {
                Starting::BRIGHTNESS as i64
            } else {
                engine.panel().brightness
            };
            if shown != brightness_shown {
                brightness_shown = shown;
                board::panel_brightness(shown.clamp(0, 255) as u8);
            }
            button.poll(boot.is_low(), t, &mut engine, &mut b);
            if RECENT_UNSAVED.swap(false, Ordering::Relaxed) {
                let text = state().recent.to_json();
                shared().store.set("recent", text.as_bytes());
            }
            next_tick = t + TICK_MS;
        }
    }
}

/// The web app's level graph, at most 10 a second.
fn level(db: f32) {
    static LAST: Mutex<Option<Instant>> = Mutex::new(None);
    let mut last = LAST.lock().unwrap();
    if last.is_none_or(|l| l.elapsed() >= Duration::from_millis(100)) {
        *last = Some(Instant::now());
        live::send(&json!({"type": "level", "db": (db * 10.0).round() / 10.0}));
    }
}
