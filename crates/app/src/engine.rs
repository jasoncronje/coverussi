//! The board's main loop logic: it feeds Home Assistant, the mic, recognition
//! answers and decoded art into the model and tells the platform what to do
//! (follow entities, fetch art, turn the mic on, record a clip, draw a frame,
//! tell the portal).

use std::collections::HashMap;
use std::sync::Arc;

use serde_json::{json, Map, Value};

use crate::art::{self, Compositor, Frame, Rgb, Strip};
use crate::ha::HaStates;
use crate::idle::{self, LocalTime};
use crate::model::{Calibration, Device, Mode, Ms, Recognition};
use crate::settings::{Color, Fit, Settings, Transition};

pub trait Platform {
    /// Subscribe to these in Home Assistant.
    fn follow(&mut self, entities: &[String]);
    /// Fetch (a Home Assistant path or a URL), then `Engine::image`.
    fn want_art(&mut self, key: &str);
    fn mic(&mut self, on: bool);
    /// Record and identify a clip; then `Engine::recognition` or `clip_failed`.
    fn record_clip(&mut self, ms: u32);
    /// A new frame to draw (and mirror).
    fn frame(&mut self, f: &Frame);
    /// How to draw colour: the saved tuning, or the one the calibration screen is trying.
    fn color(&mut self, c: &Color);
    /// A message for the portal (telemetry, an event).
    fn send(&mut self, message: Value);
    /// Commands the platform handles: restart and the like.
    fn command(&mut self, name: &str, args: &Value);
    /// Firmware, uptime, heap, Wi-Fi...
    fn telemetry_extras(&mut self, _t: &mut Map<String, Value>) {}
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct PanelState {
    /// After the current limit.
    pub brightness: i64,
    pub requested_brightness: i64,
    pub current_ma: i64,
    pub fps: f32,
}

const MAX_IMAGES: usize = 8;
const HA_ART_PROXY: &str = "/api/media_player_proxy/";
/// After the art changes. Home Assistant's art proxy can serve a placeholder for the first
/// seconds of a track (the Apple TV's grey note) and the real cover later at the same URL.
const REFETCH_MS: [Ms; 2] = [4000, 12000];
pub const TEST_PATTERN_MS: Ms = 8000;
/// The turning record, about 15 frames a second.
const RECORD_FRAME_MS: Ms = 66;
/// A calibration screen that stops calling (a closed tab) gives the panel back after this.
pub const CALIBRATION_MS: Ms = 120_000;
pub const TELEMETRY_EVERY_MS: Ms = 2000;

pub struct Engine {
    pub device: Device,
    pub settings_version: u32,
    ha: HaStates,
    compositor: Compositor,
    frame: Box<Frame>,
    images: HashMap<String, Arc<Rgb>>,
    /// With display.fit pan, the wide ones among them, to pan across.
    strips: HashMap<String, Arc<Strip>>,
    wanted: String,
    followed: Option<Vec<String>>,
    wanted_at: Ms,
    refetches: usize,
    art_rev: u32,
    mic: bool,
    test_pattern_until: Ms,
    /// Which pattern test_pattern_until is for.
    pattern: &'static str,
    /// The tuning the calibration screen is trying, until when.
    trial: Option<(Color, Ms)>,
    /// The tuning last given to the platform.
    color: Option<Color>,
    last_telemetry: Option<Ms>,
    fps_since: Ms,
    frames: u32,
    last_summary: String,
    local_min: u32,
    clock: Option<LocalTime>,
    /// What the idle screen says, while it's up (no art wanted).
    idle: Option<Option<idle::Content>>,
    /// The last song recognised: title, artist.
    song: Option<(String, String)>,
    panel: PanelState,
}

impl Default for Engine {
    fn default() -> Self {
        Self {
            device: Device::default(),
            settings_version: 0,
            ha: HaStates::default(),
            compositor: Compositor::default(),
            frame: Box::new([0; art::SIZE * art::SIZE]),
            images: HashMap::new(),
            strips: HashMap::new(),
            wanted: String::new(),
            followed: None,
            wanted_at: 0,
            refetches: 0,
            art_rev: 0,
            mic: false,
            test_pattern_until: 0,
            pattern: "",
            trial: None,
            color: None,
            last_telemetry: None,
            fps_since: 0,
            frames: 0,
            last_summary: String::new(),
            local_min: 12 * 60,
            clock: None,
            idle: None,
            song: None,
            panel: PanelState::default(),
        }
    }
}

impl Engine {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn panel(&self) -> &PanelState {
        &self.panel
    }

    pub fn set_local_min(&mut self, m: u32) {
        self.local_min = m;
    }

    /// The local time for the night hours and the idle screen; None until the clock is set (noon).
    pub fn set_clock(&mut self, c: Option<LocalTime>) {
        self.local_min = c.map_or(12 * 60, |c| c.minute);
        self.clock = c;
    }

    /// Resubscribe after a reconnect: `follow` again with the current list.
    pub fn refollow(&mut self) {
        self.followed = None;
    }

    /// The Home Assistant states the engine has seen.
    pub fn ha(&self) -> &HaStates {
        &self.ha
    }

    pub fn telemetry(&self, now: Ms) -> Map<String, Value> {
        let d = &self.device;
        let src = d.source();
        // An analog source goes by the name it was given ("Turntable"), not its trigger entity's.
        let analog = d.analog_source();
        let source_name = if d.room() {
            "Room".to_string()
        } else if let Some(a) = analog.filter(|a| !a.name.is_empty()) {
            a.name.clone()
        } else {
            src.map(|p| p.name.clone()).unwrap_or_default()
        };
        let (title, artist, album) = if d.mode() == Mode::Analog {
            let r = d.listener().last();
            let text = |s: &String| if r.matched { s.clone() } else { String::new() };
            (text(&r.title), text(&r.artist), text(&r.album))
        } else {
            let text = |f: fn(&crate::model::Player) -> &String| src.map(|p| f(p).clone()).unwrap_or_default();
            (text(|p| &p.title), text(|p| &p.artist), text(|p| &p.album))
        };
        let l = d.listener();
        let t = json!({
            "mode": d.mode().name(),
            "source": src.map(|p| p.entity.as_str()).unwrap_or(""),
            "source_name": source_name,
            "app": if analog.is_none() { src.map(|p| p.app_name.as_str()).unwrap_or("") } else { "" },
            "title": title,
            "artist": artist,
            "album": album,
            "art": d.art(),
            "art_dimmed": d.art_dimmed(),
            "panel_on": d.panel_on(),
            "brightness": self.panel.brightness,
            "requested_brightness": self.panel.requested_brightness,
            "current_ma": self.panel.current_ma,
            "fps": self.panel.fps,
            "settings_version": self.settings_version,
            "mic": d.mic_wanted(),
            "calibrating": d.calibrating(),
            "test_pattern": self.test_pattern_until != 0,
            "listener": {
                "state": l.state().name(),
                "busy": l.busy(),
                "next_identify_in_ms": l.next_identify_in(now).unwrap_or(-1),
                "failures": l.failures(),
                "given_up": l.given_up(),
                "music": d.levels().music(),
                "silent_ms": d.levels().silent_for(now),
                "level_db": d.levels().last_db(),
            },
        });
        match t {
            Value::Object(m) => m,
            _ => unreachable!(),
        }
    }

    fn send_telemetry(&mut self, now: Ms, p: &mut impl Platform) {
        let mut t = self.telemetry(now);
        p.telemetry_extras(&mut t);
        p.send(json!({"type": "telemetry", "data": t}));
        self.last_telemetry = Some(now);
    }

    /// Mode, listener state and art: a change sends telemetry at once.
    fn summary(&self) -> String {
        let d = &self.device;
        let analog = d.analog_source().map(|a| a.name.as_str()).unwrap_or(if d.room() { "room" } else { "" });
        format!(
            "{}{analog}{}{}{}{}{}{}{}{}{}",
            d.mode().name(),
            d.listener().state().name(),
            if d.listener().given_up() { "g" } else { "" },
            d.art(),
            if d.listener().busy() { "b" } else { "" },
            if d.art_dimmed() { "d" } else { "" },
            if d.panel_on() { "on" } else { "" },
            self.settings_version,
            if d.calibrating() { "c" } else { "" },
            if self.test_pattern_until != 0 { "t" } else { "" },
        )
    }

    fn show_wanted(&mut self, now: Ms) {
        if self.test_pattern_until != 0 {
            return;
        }
        let (fade, how) = (self.device.settings.display.crossfade_ms as Ms, self.device.settings.display.transition);
        if self.wanted.is_empty() {
            self.idle = None;
            self.show_empty(now);
        } else if let Some(img) = self.images.get(&self.wanted) {
            let key = format!("{}#{}", self.wanted, self.art_rev);
            self.compositor.show(Some(img.clone()), &key, now, fade, how);
            self.compositor.pan(self.strips.get(&self.wanted).cloned());
        }
    }

    /// A recognised record's cover, with its title and artist in between.
    fn show_song(&mut self, now: Ms) {
        let showing = self.compositor.key().starts_with("song ");
        let d = &self.device.settings.display;
        let (cover, text) = (d.song_cover_s as Ms * 1000, d.song_text_s as Ms * 1000);
        let song = self.song.as_ref().filter(|_| {
            d.song_screen
                && self.device.mode() == Mode::Analog
                && !self.device.art_dimmed()
                && self.images.contains_key(&self.wanted)
                && (now - self.wanted_at) % (cover + text) >= cover
        });
        match song {
            Some((title, artist)) => {
                let key = format!("song {title}\n{artist}");
                if self.compositor.key() != key {
                    let d = &self.device.settings.display;
                    self.compositor.show(Some(Arc::from(idle::song(title, artist))), &key, now, d.crossfade_ms as Ms, d.transition);
                }
            }
            None if showing => self.show_wanted(now),
            None => {}
        }
    }

    /// No art: a record turning while one plays unrecognised, else the idle screen.
    fn show_empty(&mut self, now: Ms) {
        if !self.device.settings.display.record || !self.device.unrecognised() {
            return self.show_idle(now);
        }
        let turning = self.compositor.key().starts_with("record ");
        let key = format!("record {}", now / RECORD_FRAME_MS);
        // The first frame comes in with the usual transition; the turning waits for it.
        if self.compositor.key() == key || (turning && self.compositor.animating(now)) {
            return;
        }
        let d = &self.device.settings.display;
        let (fade, how) = if turning { (0, Transition::Cut) } else { (d.crossfade_ms as Ms, d.transition) };
        let l = &self.device.settings.listening;
        let db = self.device.levels().last_db() as f64;
        let loudness = if d.record_sheen { ((db - l.gap_threshold_db) / (l.music_threshold_db + 25.0 - l.gap_threshold_db)).clamp(0.0, 1.0) } else { 0.5 };
        let label = d.record_color().unwrap_or([242, 179, 61]);
        self.compositor.show(Some(Arc::from(idle::record(now, d.record_turn_ms(), label, loudness as f32))), &key, now, fade, how);
        self.idle = None; // the idle screen comes back with a transition
    }

    /// No art: the idle screen, redrawn when what it says changes (the minute, a state).
    fn show_idle(&mut self, now: Ms) {
        let content = idle::content(&self.device.settings.idle, &self.ha, self.clock);
        if self.idle.as_ref() == Some(&content) {
            return;
        }
        let d = &self.device.settings.display;
        // From art, the usual transition; from one idle screen to the next, a cut.
        let (fade, how) = if self.idle.is_some() { (0, Transition::Cut) } else { (d.crossfade_ms as Ms, d.transition) };
        match &content {
            Some(c) => self.compositor.show(Some(Arc::from(idle::draw(c))), &format!("idle {c:?}"), now, fade, how),
            None => self.compositor.show(None, "", now, fade, how),
        }
        self.idle = Some(content);
    }

    fn step(&mut self, now: Ms, p: &mut impl Platform) {
        self.device.update(now);
        let entities = self.device.entities();
        if self.followed.as_ref() != Some(&entities) {
            p.follow(&entities);
            self.followed = Some(entities);
        }
        if self.device.art() != self.wanted {
            self.wanted = self.device.art().to_string();
            self.wanted_at = now;
            self.refetches = 0;
            if !self.wanted.is_empty() && !self.images.contains_key(&self.wanted) {
                p.want_art(&self.wanted);
            }
            self.show_wanted(now);
        }
        if self.refetches < REFETCH_MS.len()
            && self.wanted.starts_with(HA_ART_PROXY)
            && now - self.wanted_at >= REFETCH_MS[self.refetches]
        {
            self.refetches += 1;
            p.want_art(&self.wanted);
        }
        if self.wanted.is_empty() && self.test_pattern_until == 0 {
            self.show_empty(now);
        }
        if !self.wanted.is_empty() && self.test_pattern_until == 0 {
            self.show_song(now);
        }
        if self.trial.is_some_and(|(_, until)| now >= until) {
            self.end_calibration(now);
        }
        let color = self.trial.map_or(self.device.settings.display.color, |(c, _)| c);
        if self.color != Some(color) {
            self.color = Some(color);
            p.color(&color);
        }
        if self.test_pattern_until != 0 && now >= self.test_pattern_until {
            self.test_pattern_until = 0;
            self.compositor.show(None, "", now, 0, Default::default());
            self.show_wanted(now);
        }
        self.compositor.set_dim(self.device.art_dimmed() && self.test_pattern_until == 0 && !self.wanted.is_empty());
        if self.device.mic_wanted() != self.mic {
            self.mic = self.device.mic_wanted();
            p.mic(self.mic);
        }
        if self.device.take_clip_request() {
            p.record_clip(self.device.settings.listening.clip_ms as u32);
        }
        if self.device.take_analog_stopped() {
            p.send(json!({"type": "event", "name": "analog_stopped"}));
        }
        if let Some(c) = self.device.take_calibration() {
            p.send(calibration_event(&c));
        }
        let s = self.summary();
        if s != self.last_summary || self.last_telemetry.is_none_or(|t| now - t >= TELEMETRY_EVERY_MS) {
            self.last_summary = s;
            self.send_telemetry(now, p);
        }
    }

    /// About 30 a second: render and measure.
    pub fn tick(&mut self, now: Ms, p: &mut impl Platform) {
        if self.compositor.render(now, &mut self.frame) {
            p.frame(&self.frame);
            self.frames += 1;
        }
        if now - self.fps_since >= 2000 {
            self.panel.fps = self.frames as f32 * 1000.0 / (now - self.fps_since) as f32;
            self.frames = 0;
            self.fps_since = now;
        }
        let d = &self.device;
        self.panel.requested_brightness = if d.panel_on() { d.brightness(self.local_min) } else { 0 };
        self.panel.brightness =
            art::limited_brightness(&self.frame, self.panel.requested_brightness, d.settings.display.current_limit_ma);
        self.panel.current_ma = art::estimate_ma(&self.frame, self.panel.brightness);
        self.step(now, p);
    }

    pub fn settings(&mut self, s: Settings, version: u32, now: Ms, p: &mut impl Platform) {
        let refit = s.display.fit != self.device.settings.display.fit;
        self.device.set_settings(s);
        if refit {
            // The cache holds art scaled the old way: fetch it again, and fade to it when it comes.
            self.images.clear();
            self.strips.clear();
            self.art_rev += 1;
            if !self.wanted.is_empty() {
                p.want_art(&self.wanted);
            }
        }
        self.settings_version = version;
        self.step(now, p);
    }

    pub fn ha_event(&mut self, event: &Value, now: Ms, p: &mut impl Platform) {
        for entity in self.ha.apply(event) {
            let player = self.ha.player(&entity);
            self.device.player(player, now);
        }
        self.step(now, p);
    }

    pub fn level(&mut self, db: f32, now: Ms, p: &mut impl Platform) {
        self.device.level(db, now);
        self.step(now, p);
    }

    pub fn recognition(&mut self, answer: &Value, now: Ms, p: &mut impl Platform) {
        let r = parse_recognition(answer);
        if r.matched {
            self.song = Some((r.title.clone(), r.artist.clone()));
        }
        self.device.recognition(&r, now);
        self.step(now, p);
    }

    pub fn clip_failed(&mut self, now: Ms, p: &mut impl Platform) {
        self.device.clip_failed(now);
        self.step(now, p);
    }

    pub fn image(&mut self, key: &str, rgb: &[u8], w: usize, h: usize, now: Ms, p: &mut impl Platform) {
        if w == 0 || h == 0 || rgb.len() < w * h * 3 {
            return;
        }
        let fit = self.device.settings.display.fit;
        let strip = (fit == Fit::Pan).then(|| art::scale_to_strip(rgb, w, h)).flatten();
        // A pan starts from the strip's left end.
        let img: Arc<Rgb> = Arc::from(strip.as_ref().map_or_else(|| art::scale_to_square(rgb, w, h, fit), |s| s.left()));
        if self.images.len() >= MAX_IMAGES && !self.images.contains_key(key) {
            self.images.clear();
            self.strips.clear();
        }
        match strip {
            Some(strip) => self.strips.insert(key.to_string(), Arc::new(strip)),
            None => self.strips.remove(key),
        };
        let changed = self.images.get(key).is_some_and(|old| old[..] != img[..]);
        self.images.insert(key.to_string(), img);
        if changed {
            self.art_rev += 1; // a new picture behind the same URL: fade to it
        }
        if key == self.wanted {
            self.show_wanted(now);
        }
        self.step(now, p);
    }

    /// Leave the screen as it is; a later change of art tries again.
    pub fn image_failed(&mut self, now: Ms, p: &mut impl Platform) {
        self.step(now, p);
    }

    pub fn command(&mut self, name: &str, args: &Value, now: Ms, p: &mut impl Platform) {
        match name {
            "test_pattern" => {
                // {"on": false} ends it now; otherwise it shows for TEST_PATTERN_MS.
                if args.get("on") == Some(&Value::Bool(false)) {
                    if self.test_pattern_until != 0 {
                        self.test_pattern_until = now;
                    }
                } else {
                    self.show_pattern("test pattern", art::test_pattern, now);
                    self.test_pattern_until = now + TEST_PATTERN_MS;
                }
            }
            "panel_calibration" => {
                // {"on": false} ends it. Otherwise {"color": {...}, "image": "pattern" | "cover"}
                // tries a tuning on the calibration image or the cover, for CALIBRATION_MS from each call.
                if args.get("on") == Some(&Value::Bool(false)) {
                    self.end_calibration(now);
                } else {
                    let saved = self.device.settings.display.color;
                    let color = args
                        .get("color")
                        .and_then(|c| serde_json::from_value::<Color>(c.clone()).ok())
                        .filter(|c| c.validate().is_ok())
                        .or(self.trial.map(|(c, _)| c))
                        .unwrap_or(saved);
                    self.trial = Some((color, now + CALIBRATION_MS));
                    if args.get("image").and_then(Value::as_str) == Some("cover") {
                        if self.pattern == "calibration" {
                            self.test_pattern_until = now;
                        }
                    } else {
                        self.show_pattern("calibration", art::calibration_image, now);
                        self.test_pattern_until = now + CALIBRATION_MS;
                    }
                }
            }
            "apps" => {
                // {"entity": ...}; without one, the first digital source with an app list, else the first one.
                let digital = &self.device.settings.sources.digital;
                let entity = args
                    .get("entity")
                    .and_then(Value::as_str)
                    .map(String::from)
                    .or_else(|| digital.iter().find(|d| !d.apps.is_empty()).map(|d| d.entity.clone()))
                    .or_else(|| digital.first().map(|d| d.entity.clone()))
                    .unwrap_or_default();
                p.send(json!({"type": "event", "name": "apps_result", "data": self.ha.apps(&entity)}));
            }
            _ => {
                if !apply_command(&mut self.device, name, args, now) {
                    p.command(name, args);
                }
            }
        }
        self.step(now, p);
    }

    fn show_pattern(&mut self, name: &'static str, image: fn() -> Box<Rgb>, now: Ms) {
        if self.test_pattern_until == 0 || self.pattern != name {
            self.compositor.show(Some(Arc::from(image())), name, now, 0, Default::default());
            self.pattern = name;
        }
    }

    /// The saved tuning back, and the art if the calibration image is up.
    fn end_calibration(&mut self, now: Ms) {
        self.trial = None;
        if self.test_pattern_until != 0 && self.pattern == "calibration" {
            self.test_pattern_until = now;
        }
    }

    /// Send telemetry at once (a portal just connected).
    pub fn announce(&mut self, now: Ms, p: &mut impl Platform) {
        self.send_telemetry(now, p);
    }
}

/// A recognition answer, as `recognize` makes it.
pub fn parse_recognition(a: &Value) -> Recognition {
    let text = |k: &str| a.get(k).and_then(Value::as_str).unwrap_or_default().to_string();
    let matched = a.get("match").and_then(Value::as_bool).unwrap_or(false);
    let mut reason = text("reason");
    if !matched && reason.is_empty() {
        reason = "error".into();
    }
    Recognition {
        matched,
        reason,
        title: text("title"),
        artist: text("artist"),
        album: text("album"),
        art_url: text("art_url"),
        ends_in_ms: a.get("ends_in_ms").and_then(Value::as_f64).filter(|v| *v >= 0.0).map(|v| v as u64),
        retry_in_ms: a.get("retry_in_ms").and_then(Value::as_f64).map_or(0, |v| v.max(0.0) as u64),
    }
}

/// Commands the model handles: identify, panel, calibrate. False for any other.
pub fn apply_command(d: &mut Device, name: &str, args: &Value, now: Ms) -> bool {
    match name {
        "identify" => d.identify_now(),
        "panel" => {
            if let Some(on) = args.get("on").and_then(Value::as_bool) {
                d.set_panel(on);
            }
            if let Some(b) = args.get("brightness").and_then(Value::as_f64) {
                d.set_brightness_override((b >= 0.0).then(|| (b as i64).min(255)));
            }
        }
        "calibrate" => {
            let seconds = args.get("seconds").and_then(Value::as_f64).map_or(8, |s| (s as u64).clamp(2, 30));
            d.start_calibration(now, seconds * 1000);
        }
        _ => return false,
    }
    true
}

pub fn calibration_event(c: &Calibration) -> Value {
    json!({"type": "event", "name": "calibrate_result",
           "data": {"p10": c.p10, "median": c.median, "p90": c.p90, "samples": c.samples}})
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::tests::house_settings;

    /// A platform that writes down what the engine asks for.
    #[derive(Default)]
    struct Recorder {
        followed: Vec<String>,
        art: Vec<String>,
        sent: Vec<Value>,
        commands: Vec<String>,
        mics: Vec<bool>,
        clips: Vec<u32>,
        frames: usize,
        last: Option<Box<Frame>>,
        colors: Vec<Color>,
    }

    impl Platform for Recorder {
        fn follow(&mut self, e: &[String]) {
            self.followed = e.to_vec();
        }
        fn want_art(&mut self, key: &str) {
            self.art.push(key.into());
        }
        fn mic(&mut self, on: bool) {
            self.mics.push(on);
        }
        fn record_clip(&mut self, ms: u32) {
            self.clips.push(ms);
        }
        fn frame(&mut self, f: &Frame) {
            self.frames += 1;
            self.last = Some(Box::new(*f));
        }
        fn color(&mut self, c: &Color) {
            self.colors.push(*c);
        }
        fn send(&mut self, m: Value) {
            self.sent.push(m);
        }
        fn command(&mut self, name: &str, _: &Value) {
            self.commands.push(name.into());
        }
        fn telemetry_extras(&mut self, t: &mut Map<String, Value>) {
            t.insert("firmware".into(), "test".into());
        }
    }

    impl Recorder {
        fn last_event(&self, name: &str) -> &Value {
            self.sent.iter().rev().find(|m| m["name"] == name).expect(name)
        }
    }

    fn engine() -> Engine {
        let mut e = Engine::new();
        e.device.set_settings(house_settings());
        e
    }

    fn red(n: usize) -> Vec<u8> {
        (0..n * n).flat_map(|_| [255, 0, 0]).collect()
    }

    #[test]
    fn lists_a_digital_sources_apps() {
        let (mut r, mut e) = (Recorder::default(), engine());
        e.tick(1, &mut r);
        let tv = json!({"entity": "media_player.apple_tv"});
        e.command("apps", &tv, 2, &mut r);
        let m = r.last_event("apps_result");
        assert_eq!((m["data"]["entity"].as_str(), m["data"]["followed"].as_bool()), (Some("media_player.apple_tv"), Some(false)));
        e.ha_event(&json!({"a": {"media_player.apple_tv": {"s": "playing", "a": {"app_name": "Music",
            "app_id": "com.apple.TVMusic", "source": "Music", "source_list": ["Music", "Netflix", "YouTube"]}}}}), 3, &mut r);
        e.command("apps", &tv, 4, &mut r);
        let d = &r.last_event("apps_result")["data"];
        assert_eq!(d["followed"], true);
        assert_eq!(d["app_id"], "com.apple.TVMusic");
        assert_eq!(d["apps"], json!(["Music", "Netflix", "YouTube"]));
        assert!(r.commands.is_empty()); // the engine answers it, not the platform
        // No entity: the first player with an app list, else the first player.
        e.command("apps", &Value::Null, 5, &mut r);
        assert_eq!(r.last_event("apps_result")["data"]["entity"], "media_player.wiim_ultra_2");
        let mut s = house_settings();
        s.sources.digital[1].apps = vec!["Music".into()];
        e.device.set_settings(s);
        e.command("apps", &Value::Null, 6, &mut r);
        assert_eq!(r.last_event("apps_result")["data"]["entity"], "media_player.apple_tv");
        // No digital sources at all: an empty answer, not a crash.
        e.device.set_settings(Settings::default());
        e.command("apps", &Value::Null, 7, &mut r);
        assert_eq!(r.last_event("apps_result")["data"]["entity"], "");
    }

    #[test]
    fn refetches_proxied_art_that_was_a_placeholder() {
        let (mut r, mut e) = (Recorder::default(), engine());
        let mut t = 1;
        e.tick(t, &mut r);
        let url = "/api/media_player_proxy/media_player.apple_tv?token=1&cache=A";
        e.ha_event(&json!({"a": {"media_player.apple_tv": {"s": "playing", "a": {"app_name": "Spotify",
            "media_title": "Come Thru", "media_artist": "ROM COM", "entity_picture": url}}}}), t, &mut r);
        assert_eq!(r.art, [url]);
        e.image(url, &[40; 8 * 8 * 3], 8, 8, t, &mut r);
        for _ in 0..100 {
            t += 33;
            e.tick(t, &mut r);
        }
        assert_eq!(r.art.len(), 1); // 3.3 s: not yet
        for _ in 0..30 {
            t += 33;
            e.tick(t, &mut r);
        }
        assert_eq!(r.art, [url, url]);
        let placeholder = r.last.clone().unwrap();
        e.image(url, &red(8), 8, 8, t, &mut r);
        for _ in 0..60 {
            t += 33;
            e.tick(t, &mut r);
        }
        assert_ne!(r.last.clone().unwrap()[..], placeholder[..]);
        for _ in 0..300 {
            t += 33;
            e.tick(t, &mut r);
        }
        assert_eq!(r.art.len(), 3); // once more at 12 s, then it stops asking
        // Art from the internet (a matched record) is never fetched twice.
        let (mut r2, mut e2) = (Recorder::default(), engine());
        e2.tick(1, &mut r2);
        e2.ha_event(&json!({"a": {"media_player.wiim_ultra_2": {"s": "playing", "a": {"source": "Network",
            "entity_picture": "https://art/peg.jpg"}}}}), 1, &mut r2);
        let mut u = 1;
        while u < 20000 {
            e2.tick(u, &mut r2);
            u += 33;
        }
        assert_eq!(r2.art.len(), 1);
    }

    #[test]
    fn test_pattern_ends_on_its_own_or_when_switched_off() {
        let (mut r, mut e) = (Recorder::default(), engine());
        let mut t = 1;
        e.tick(t, &mut r);
        e.ha_event(&json!({"a": {"media_player.wiim_ultra_2": {"s": "playing", "a": {"source": "Network",
            "entity_picture": "https://art/peg.jpg"}}}}), t, &mut r);
        e.image("https://art/peg.jpg", &red(8), 8, 8, t, &mut r);
        let run = |e: &mut Engine, r: &mut Recorder, n: usize, t: &mut Ms| {
            for _ in 0..n {
                *t += 33;
                e.tick(*t, r);
            }
            r.last.clone().unwrap()
        };
        let art = run(&mut e, &mut r, 100, &mut t);
        e.command("test_pattern", &Value::Null, t, &mut r);
        assert_ne!(run(&mut e, &mut r, 30, &mut t)[..], art[..]);
        assert_eq!(run(&mut e, &mut r, 300, &mut t)[..], art[..]);
        e.command("test_pattern", &Value::Null, t, &mut r);
        assert_ne!(run(&mut e, &mut r, 30, &mut t)[..], art[..]);
        e.command("test_pattern", &json!({"on": false}), t, &mut r);
        assert_eq!(run(&mut e, &mut r, 60, &mut t)[..], art[..]); // 2 s: well inside the 8
        let last = r.sent.iter().rev().find(|m| m["type"] == "telemetry").unwrap();
        assert_eq!(last["data"]["test_pattern"], false);
    }

    #[test]
    fn calibration_tries_a_colour_then_gives_the_panel_back() {
        let (mut r, mut e) = (Recorder::default(), engine());
        let mut t = 1;
        e.tick(t, &mut r);
        e.ha_event(&json!({"a": {"media_player.wiim_ultra_2": {"s": "playing", "a": {"source": "Network",
            "entity_picture": "https://art/peg.jpg"}}}}), t, &mut r);
        e.image("https://art/peg.jpg", &red(8), 8, 8, t, &mut r);
        let run = |e: &mut Engine, r: &mut Recorder, n: usize, t: &mut Ms| {
            for _ in 0..n {
                *t += 33;
                e.tick(*t, r);
            }
            r.last.clone().unwrap()
        };
        let art = run(&mut e, &mut r, 100, &mut t);
        assert_eq!(r.colors, [Color::default()]);
        let warm = Color { blue_pct: 80, ..Color::default() };
        e.command("panel_calibration", &json!({"color": warm}), t, &mut r);
        assert_eq!(r.colors.last(), Some(&warm));
        assert_ne!(run(&mut e, &mut r, 30, &mut t)[..], art[..]); // the calibration image
        // A bad tuning keeps the last good one; the cover can stand in for the image.
        e.command("panel_calibration", &json!({"color": {"shadows": 99}, "image": "cover"}), t, &mut r);
        assert_eq!(r.colors.last(), Some(&warm));
        assert_eq!(run(&mut e, &mut r, 30, &mut t)[..], art[..]);
        e.command("panel_calibration", &json!({"on": false}), t, &mut r);
        assert_eq!(r.colors.last(), Some(&Color::default()));
        // A screen that stops calling gives the panel back on its own.
        e.command("panel_calibration", &json!({"color": warm}), t, &mut r);
        run(&mut e, &mut r, (CALIBRATION_MS / 33) as usize + 2, &mut t);
        assert_eq!(r.colors.last(), Some(&Color::default()));
        assert_eq!(run(&mut e, &mut r, 30, &mut t)[..], art[..]); // after the fade back
    }

    #[test]
    fn runs_a_record() {
        let (mut r, mut e) = (Recorder::default(), engine());
        let mut t = 1;
        e.tick(t, &mut r);
        assert_eq!(r.followed.len(), 2); // the WiiM and the Apple TV
        assert_eq!(r.sent.last().unwrap()["data"]["firmware"], "test");
        e.ha_event(&json!({"a": {"media_player.wiim_ultra_2": {"s": "idle", "a": {"source": "Phono In"}}}}), t, &mut r);
        assert_eq!(r.mics, [true]);
        while t < 1600 {
            e.level(-30.0, t, &mut r);
            t += 50;
        }
        assert_eq!(r.clips, [e.device.settings.listening.clip_ms as u32]);
        e.recognition(&json!({"match": true, "title": "Dreams", "artist": "Fleetwood Mac", "album": "Rumours",
            "art_url": "https://art/rumours.jpg", "ends_in_ms": 200000}), t, &mut r);
        assert_eq!(r.art, ["https://art/rumours.jpg"]);
        e.image("https://art/rumours.jpg", &red(8), 8, 8, t, &mut r);
        let before = r.frames;
        for _ in 0..30 {
            t += 33;
            e.tick(t, &mut r);
        }
        assert!(r.frames > before);
        assert!(e.panel().brightness > 0 && e.panel().current_ma > art::IDLE_MA);
        let tele = r.sent.iter().rev().find(|m| m["type"] == "telemetry").unwrap();
        assert_eq!(tele["data"]["title"], "Dreams");
        assert_eq!(tele["data"]["source_name"], "Turntable");
        e.command("restart", &Value::Null, t, &mut r);
        assert_eq!(r.commands, ["restart"]);
        e.command("panel", &json!({"on": false}), t, &mut r);
        t += 33;
        e.tick(t, &mut r);
        assert_eq!(e.panel().brightness, 0);
        t += 33;
        e.ha_event(&json!({"c": {"media_player.wiim_ultra_2": {"+": {"a": {"source": "Network"}}}}}), t, &mut r);
        assert_eq!(r.mics.last(), Some(&false));
        assert!(r.sent.iter().any(|m| m["name"] == "analog_stopped"));
    }

    #[test]
    fn a_record_turns_until_it_is_recognised() {
        let (mut r, mut e) = (Recorder::default(), engine());
        let mut t = 1;
        e.ha_event(&json!({"a": {"media_player.wiim_ultra_2": {"s": "idle", "a": {"source": "Phono In"}}}}), t, &mut r);
        while t < 1600 {
            e.level(-30.0, t, &mut r);
            t += 50;
        }
        e.recognition(&json!({"match": false, "reason": "no_match"}), t, &mut r);
        let mut frames = Vec::new();
        for _ in 0..40 {
            t += 33;
            e.tick(t, &mut r);
            frames.push(r.last.clone().unwrap());
        }
        assert!(frames[39].iter().any(|&p| p != 0));
        assert_ne!(frames[30][..], frames[39][..]); // turning
        e.recognition(&json!({"match": true, "title": "Dreams", "artist": "Fleetwood Mac", "album": "Rumours",
            "art_url": "https://art/rumours.jpg", "ends_in_ms": 200000}), t, &mut r);
        e.image("https://art/rumours.jpg", &red(8), 8, 8, t, &mut r);
        for _ in 0..30 {
            t += 33;
            e.tick(t, &mut r);
        }
        let art = r.last.clone().unwrap();
        assert!(art.iter().all(|&p| p == art[0]) && art[0] != 0);
    }

    #[test]
    fn the_record_and_the_title_can_be_turned_off() {
        let (mut r, mut e) = (Recorder::default(), Engine::new());
        let mut s = house_settings();
        (s.display.record, s.display.song_screen) = (false, false);
        e.device.set_settings(s);
        let mut t = 1;
        e.ha_event(&json!({"a": {"media_player.wiim_ultra_2": {"s": "idle", "a": {"source": "Phono In"}}}}), t, &mut r);
        while t < 1600 {
            e.level(-30.0, t, &mut r);
            t += 50;
        }
        e.recognition(&json!({"match": false, "reason": "no_match"}), t, &mut r);
        for _ in 0..40 {
            t += 33;
            e.tick(t, &mut r);
        }
        assert!(!e.compositor.key().starts_with("record "));
        e.recognition(&json!({"match": true, "title": "Dreams", "artist": "Fleetwood Mac", "album": "Rumours",
            "art_url": "https://art/rumours.jpg", "ends_in_ms": 200000}), t, &mut r);
        e.image("https://art/rumours.jpg", &red(8), 8, 8, t, &mut r);
        let start = t;
        while t < start + 25_000 {
            t += 33;
            e.level(-30.0, t, &mut r);
            e.tick(t, &mut r);
        }
        let art = r.last.clone().unwrap();
        assert!(art.iter().all(|&p| p == art[0]) && art[0] != 0); // the cover, no title
    }

    #[test]
    fn a_recognised_record_shows_its_title_between_covers() {
        let (mut r, mut e) = (Recorder::default(), engine());
        let mut t = 1;
        e.ha_event(&json!({"a": {"media_player.wiim_ultra_2": {"s": "idle", "a": {"source": "Phono In"}}}}), t, &mut r);
        while t < 1600 {
            e.level(-30.0, t, &mut r);
            t += 50;
        }
        e.recognition(&json!({"match": true, "title": "Dreams", "artist": "Fleetwood Mac", "album": "Rumours",
            "art_url": "https://art/rumours.jpg", "ends_in_ms": 200000}), t, &mut r);
        e.image("https://art/rumours.jpg", &red(8), 8, 8, t, &mut r);
        let start = t;
        let mut at = |e: &mut Engine, r: &mut Recorder, ms: Ms| {
            while t < start + ms {
                t += 33;
                e.level(-30.0, t, r);
                e.tick(t, r);
            }
            r.last.clone().unwrap()
        };
        let cover = at(&mut e, &mut r, 5_000);
        assert!(cover.iter().all(|&p| p == cover[0]) && cover[0] != 0);
        let song = at(&mut e, &mut r, 22_000);
        assert!(song.contains(&0) && song.iter().any(|&p| p != 0)); // text on black
        assert_eq!(at(&mut e, &mut r, 28_000)[..], cover[..]); // and back
    }

    #[test]
    fn follows_the_new_sources() {
        let (mut r, mut e) = (Recorder::default(), Engine::new());
        e.tick(1, &mut r);
        assert!(r.followed.is_empty());
        let s = Settings::from_json(r#"{"sources":{"digital":[{"entity":"media_player.sonos"}],
            "analog":[{"name":"Turntable","entity":"switch.turntable","attribute":"state","value":"on"}]}}"#).unwrap();
        e.settings(s, 1, 2, &mut r);
        assert_eq!(r.followed, ["media_player.sonos", "switch.turntable"]);
        e.ha_event(&json!({"a": {"switch.turntable": {"s": "on", "a": {}}}}), 3, &mut r);
        assert_eq!(r.mics.last(), Some(&true));
    }

    #[test]
    fn commands_and_calibration() {
        let (mut r, mut e) = (Recorder::default(), engine());
        e.command("panel", &json!({"on": false}), 0, &mut r);
        assert!(!e.device.panel_on() && e.device.brightness(720) == 0);
        e.command("calibrate", &json!({"seconds": 3}), 0, &mut r);
        assert!(e.device.mic_wanted());
        let mut t = 0;
        while t <= 3000 {
            e.level(if t < 1500 { -60.0 } else { -30.0 }, t, &mut r);
            t += 50;
        }
        let c = &r.last_event("calibrate_result")["data"];
        assert_eq!((c["p10"].as_f64(), c["p90"].as_f64(), c["samples"].as_u64()), (Some(-60.0), Some(-30.0), Some(61)));
    }

    #[test]
    fn idle_screen_shows_without_art() {
        let (mut r, mut e) = (Recorder::default(), engine());
        let mut s = house_settings();
        s.idle.show = crate::settings::IdleShow::Clock;
        e.device.set_settings(s);
        let run = |e: &mut Engine, r: &mut Recorder, n: usize, t: &mut Ms| {
            for _ in 0..n {
                *t += 33;
                e.tick(*t, r);
            }
            r.last.clone().unwrap()
        };
        let mut t = 1;
        e.set_clock(Some(LocalTime { minute: 14 * 60 + 5, weekday: 2, day: 3, month: 11 }));
        let clock = run(&mut e, &mut r, 30, &mut t);
        assert!(clock.iter().any(|&p| p != 0));
        // The next minute redraws it.
        e.set_clock(Some(LocalTime { minute: 14 * 60 + 6, weekday: 2, day: 3, month: 11 }));
        assert_ne!(run(&mut e, &mut r, 2, &mut t)[..], clock[..]);
        // Art takes over, and the clock comes back when it goes.
        e.ha_event(&json!({"a": {"media_player.wiim_ultra_2": {"s": "playing", "a": {"source": "Network",
            "entity_picture": "https://art/peg.jpg"}}}}), t, &mut r);
        e.image("https://art/peg.jpg", &red(8), 8, 8, t, &mut r);
        let art = run(&mut e, &mut r, 30, &mut t);
        assert!(art.iter().all(|&p| p == art[0]) && art[0] != 0);
        e.ha_event(&json!({"c": {"media_player.wiim_ultra_2": {"+": {"s": "off"}, "-": {"a": ["entity_picture"]}}}}), t, &mut r);
        let back = run(&mut e, &mut r, 200, &mut t);
        assert!(back.contains(&0) && back.iter().any(|&p| p != 0));
        // Off: dark.
        let mut s = house_settings();
        s.idle.show = crate::settings::IdleShow::Off;
        e.settings(s, 2, t, &mut r);
        assert!(run(&mut e, &mut r, 30, &mut t).iter().all(|&p| p == 0));
    }

    #[test]
    fn recognition_answers() {
        let r = parse_recognition(&json!({"match": true, "title": "Dreams", "album": "Rumours", "art_url": "https://a", "ends_in_ms": 1234}));
        assert!(r.matched && r.album == "Rumours" && r.ends_in_ms == Some(1234));
        let r = parse_recognition(&json!({"match": false, "reason": "rate_limited", "retry_in_ms": 60000}));
        assert!(!r.matched && r.reason == "rate_limited" && r.retry_in_ms == 60000);
        assert_eq!(parse_recognition(&Value::Null).reason, "error");
    }

    #[test]
    fn telemetry_names_the_analog_source() {
        let mut e = Engine::new();
        let mut s = Settings::default();
        s.sources.analog = vec![crate::settings::AnalogSource {
            name: "Turntable".into(),
            entity: "media_player.wiim".into(),
            attribute: "source".into(),
            value: "Phono In".into(),
            enabled: true,
        }];
        e.device.set_settings(s.clone());
        let mut r = Recorder::default();
        e.ha_event(&json!({"a": {"media_player.wiim": {"s": "idle", "a": {"source": "Phono In",
            "friendly_name": "WiiM Ultra", "app_name": "Phono"}}}}), 0, &mut r);
        let t = e.telemetry(0);
        assert_eq!((t["mode"].as_str(), t["source_name"].as_str(), t["app"].as_str()), (Some("analog"), Some("Turntable"), Some("")));
        s.sources.analog[0].name.clear();
        e.device.set_settings(s);
        e.device.update(1);
        assert_eq!(e.telemetry(1)["source_name"], "WiiM Ultra");
    }
}
