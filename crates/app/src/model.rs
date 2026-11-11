//! What the panel shows and when it listens: the platform feeds it Home
//! Assistant states, mic levels, recognition answers and the time, and carries
//! out what it asks for (fetch art, record a clip).

use std::collections::HashMap;

use crate::settings::{AnalogSource, AppShow, IdleShow, Settings};

/// Monotonic milliseconds.
pub type Ms = u64;

/// A media player (or any entity) as Home Assistant reports it.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Player {
    pub entity: String,
    /// Home Assistant's friendly name.
    pub name: String,
    pub state: String,
    pub source: String,
    pub picture: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    /// The current app (Apple TV, Android TV...).
    pub app_name: String,
    pub app_id: String,
    /// Seconds into the media, as of `position_at` (Unix seconds).
    pub position: Option<f64>,
    pub position_at: Option<f64>,
    /// The media's length in seconds.
    pub duration: Option<f64>,
    /// Every string attribute, for analog sources'.
    pub attributes: HashMap<String, String>,
    /// When the state last changed.
    pub changed: Ms,
}

impl Player {
    /// Buffering is a stream between tracks or catching up: as good as playing.
    pub fn playing(&self) -> bool {
        self.state == "playing" || self.state == "buffering"
    }

    /// Reports a song: a WiiM on its HDMI input plays "Unknown" by "Unknown".
    pub fn has_song(&self) -> bool {
        !matches!(self.title.trim(), "" | "Unknown")
    }

    pub fn usable(&self) -> bool {
        !matches!(self.state.as_str(), "" | "unavailable" | "unknown" | "off")
    }

    /// Seconds into the media at this Unix time: running on while it plays, held while paused.
    pub fn position_now(&self, unix: f64) -> Option<f64> {
        let position = self.position?;
        let since = match self.position_at {
            Some(at) if self.state == "playing" => (unix - at).max(0.0),
            _ => 0.0,
        };
        let now = position + since;
        Some(self.duration.filter(|d| *d > 0.0).map_or(now, |d| now.min(d)))
    }

    /// A string attribute ("source" falls back to the field, for players built without the map).
    pub fn attribute(&self, key: &str) -> &str {
        match self.attributes.get(key) {
            Some(v) => v,
            None if key == "source" => &self.source,
            None => "",
        }
    }
}

impl AnalogSource {
    pub fn on(&self, p: &Player) -> bool {
        if !self.enabled || self.entity.is_empty() || p.entity != self.entity || self.value.is_empty() {
            return false;
        }
        if self.attribute == "state" {
            return p.state == self.value;
        }
        p.usable() && p.attribute(&self.attribute) == self.value
    }
}

impl Settings {
    /// Brightness for a local time of day (minutes after midnight).
    pub fn brightness_at(&self, local_min: u32) -> i64 {
        if !self.display.brightness_schedule {
            return self.display.brightness_day;
        }
        let (start, end) = (minutes(&self.display.night_start, 22 * 60), minutes(&self.display.night_end, 7 * 60));
        let night = if start <= end {
            local_min >= start && local_min < end
        } else {
            local_min >= start || local_min < end // spans midnight
        };
        if night { self.display.brightness_night } else { self.display.brightness_day }
    }

    /// What this player's current app or input shows: the list it's on, else the source's `other`.
    pub fn app_show(&self, p: &Player) -> AppShow {
        let Some(d) = self.sources.digital.iter().find(|d| d.entity == p.entity) else { return AppShow::Cover };
        let same = |a: &str, b: &str| !b.is_empty() && a.eq_ignore_ascii_case(b);
        let listed = |list: &[String]| list.iter().any(|a| same(a, &p.app_name) || same(a, &p.app_id) || same(a, &p.source));
        if listed(&d.progress) {
            AppShow::Progress
        } else if listed(&d.off) {
            AppShow::Off
        } else if listed(&d.apps) {
            if d.exclude { AppShow::Off } else { AppShow::Cover }
        } else {
            d.other.unwrap_or(if d.apps.is_empty() || d.exclude { AppShow::Cover } else { AppShow::Off })
        }
    }
}

fn mode_for(show: AppShow) -> Mode {
    if show == AppShow::Progress { Mode::Progress } else { Mode::Digital }
}

/// "HH:MM" as minutes after midnight.
fn minutes(clock: &str, fallback: u32) -> u32 {
    let parse = || {
        let (h, m) = clock.split_once(':')?;
        let (h, m): (u32, u32) = (h.parse().ok()?, m.parse().ok()?);
        (h < 24 && m < 60).then_some(h * 60 + m)
    };
    parse().unwrap_or(fallback)
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Mode {
    #[default]
    Off,
    Digital,
    Analog,
    /// An app on the progress screen, no cover.
    Progress,
}

impl Mode {
    pub fn name(self) -> &'static str {
        match self {
            Mode::Off => "off",
            Mode::Digital => "digital",
            Mode::Analog => "analog",
            Mode::Progress => "progress",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LevelEvent {
    None,
    MusicStarted,
    Gap,
    SideEnd,
}

/// Turns 50 ms mic levels into what matters on an analog source: music
/// starting, a gap between tracks, the end of a side.
#[derive(Clone, Debug)]
pub struct Level {
    music: bool,
    /// Nothing heard yet counts as between sides.
    side_ended: bool,
    /// First quiet block of the current silence (0: never heard anything).
    quiet_since: Ms,
    /// First loud block of the current run of music-like sound (0: none).
    loud_since: Ms,
    /// Blocks since loud_since, and how many were loud.
    blocks: u32,
    loud_blocks: u32,
    /// First block of the current run of quiet ones, before music is confirmed.
    quiet_run: Ms,
    /// Last quiet block while music.
    last_quiet: Ms,
    last_db: f32,
}

impl Default for Level {
    fn default() -> Self {
        Self {
            music: false,
            side_ended: true,
            quiet_since: 0,
            loud_since: 0,
            blocks: 0,
            loud_blocks: 0,
            quiet_run: 0,
            last_quiet: 0,
            last_db: -120.0,
        }
    }
}

impl Level {
    /// Loud this long before it counts as music.
    pub const MUSIC_CONFIRM_MS: Ms = 1000;

    /// `confirm_ms`: how long loud counts as music (longer when listening to the whole room).
    pub fn feed(&mut self, db: f32, now: Ms, s: &Settings, confirm_ms: Ms) -> LevelEvent {
        let l = &s.listening;
        self.last_db = db;
        let loud = db as f64 >= l.music_threshold_db;
        let quiet = (db as f64) < l.gap_threshold_db;
        let side_end_ms = l.side_end_silence_s as Ms * 1000;
        let min_gap_ms = l.min_gap_ms as Ms;
        if !self.music {
            if self.quiet_since == 0 {
                self.quiet_since = now; // silence until proven otherwise
            }
            // Music has quiet moments: only a real silence (a gap) starts the count
            // again, and it confirms once a third of the blocks were loud.
            if quiet {
                if self.quiet_run == 0 {
                    self.quiet_run = now;
                }
                if now - self.quiet_run >= min_gap_ms {
                    self.loud_since = 0;
                }
            } else {
                self.quiet_run = 0;
            }
            if loud && self.loud_since == 0 {
                (self.loud_since, self.blocks, self.loud_blocks) = (now, 0, 0);
            }
            if self.loud_since != 0 {
                self.blocks += 1;
                self.loud_blocks += loud as u32;
            }
            // Music back after a short gap is the next track: the room's longer wait
            // is for music starting from silence.
            let silence = self.loud_since.saturating_sub(self.quiet_since);
            let new_side = self.side_ended || silence >= side_end_ms;
            let confirm_ms = if new_side { confirm_ms } else { confirm_ms.min(Self::MUSIC_CONFIRM_MS) };
            if self.loud_since != 0 && now - self.loud_since >= confirm_ms {
                if self.loud_blocks * 3 < self.blocks {
                    // Mostly not loud: a cough or a door, not music. Start over.
                    self.loud_since = 0;
                } else {
                    self.music = true;
                    (self.last_quiet, self.quiet_run) = (0, 0);
                    self.side_ended = false;
                    if new_side {
                        return LevelEvent::MusicStarted;
                    }
                    return if silence >= min_gap_ms { LevelEvent::Gap } else { LevelEvent::None };
                }
            }
            if !self.side_ended && !loud && now - self.quiet_since >= side_end_ms {
                self.side_ended = true;
                return LevelEvent::SideEnd;
            }
            return LevelEvent::None;
        }
        // Music: a run of quiet blocks at least min_gap long is a silence.
        if quiet {
            if self.last_quiet == 0 {
                self.last_quiet = now;
            }
            if now - self.last_quiet >= min_gap_ms {
                self.music = false;
                self.quiet_since = self.last_quiet;
                self.loud_since = 0;
            }
        } else {
            self.last_quiet = 0;
        }
        LevelEvent::None
    }

    pub fn music(&self) -> bool {
        self.music
    }

    pub fn last_db(&self) -> f32 {
        self.last_db
    }

    pub fn silent_for(&self, now: Ms) -> Ms {
        if self.music || self.quiet_since == 0 { 0 } else { now - self.quiet_since }
    }
}

/// The answer to a clip.
#[derive(Clone, Debug, PartialEq)]
pub struct Recognition {
    pub matched: bool,
    pub reason: String,
    pub title: String,
    pub artist: String,
    pub album: String,
    pub art_url: String,
    /// None: unknown.
    pub ends_in_ms: Option<u64>,
    pub retry_in_ms: u64,
}

impl Default for Recognition {
    fn default() -> Self {
        Self {
            matched: false,
            reason: "error".into(),
            title: String::new(),
            artist: String::new(),
            album: String::new(),
            art_url: String::new(),
            ends_in_ms: None,
            retry_in_ms: 0,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ListenerState {
    Quiet,
    Listening,
    Playing,
}

impl ListenerState {
    pub fn name(self) -> &'static str {
        match self {
            ListenerState::Quiet => "quiet",
            ListenerState::Listening => "listening",
            ListenerState::Playing => "playing",
        }
    }
}

/// Listening to an analog source.
#[derive(Clone, Debug)]
pub struct Listener {
    state: ListenerState,
    /// A clip is being recorded or identified.
    busy: bool,
    requested: bool,
    /// 0: none scheduled.
    next_identify: Ms,
    /// After a side ends, when the dimmed art goes.
    clear_at: Ms,
    /// No-matches in a row.
    failures: u32,
    dimmed: bool,
    room: bool,
    given_up: bool,
    art_url: String,
    last: Recognition,
}

impl Default for Listener {
    fn default() -> Self {
        Self {
            state: ListenerState::Quiet,
            busy: false,
            requested: false,
            next_identify: 0,
            clear_at: 0,
            failures: 0,
            dimmed: false,
            room: false,
            given_up: false,
            art_url: String::new(),
            last: Recognition::default(),
        }
    }
}

impl Listener {
    fn request(&mut self) {
        if self.busy {
            return;
        }
        self.busy = true;
        self.requested = true;
        self.next_identify = 0;
    }

    /// True once per clip the platform should record and identify.
    pub fn take_clip_request(&mut self) -> bool {
        std::mem::take(&mut self.requested)
    }

    pub fn identify_now(&mut self) {
        if self.state == ListenerState::Quiet {
            self.state = ListenerState::Listening;
        }
        self.failures = 0;
        self.given_up = false;
        self.request();
    }

    pub fn level_event(&mut self, e: LevelEvent, now: Ms, s: &Settings) {
        if self.given_up && e == LevelEvent::Gap {
            return; // the same noise carrying on
        }
        match e {
            LevelEvent::MusicStarted | LevelEvent::Gap => {
                if e == LevelEvent::MusicStarted {
                    self.given_up = false;
                }
                // A new side or a new track: identify again. A dimmed album from the
                // last side stays until the new match replaces it.
                if self.state == ListenerState::Quiet {
                    self.state = if self.art_url.is_empty() { ListenerState::Listening } else { ListenerState::Playing };
                }
                self.failures = 0;
                self.request();
            }
            LevelEvent::SideEnd => {
                self.state = ListenerState::Quiet;
                self.next_identify = 0;
                if !self.art_url.is_empty() {
                    self.dimmed = true;
                    self.clear_at = now + s.display.side_end_dim_s as Ms * 1000;
                }
            }
            LevelEvent::None => {}
        }
    }

    pub fn result(&mut self, r: &Recognition, now: Ms, s: &Settings) {
        self.busy = false;
        self.last = r.clone();
        if r.matched {
            self.failures = 0;
            self.art_url = r.art_url.clone();
            self.dimmed = false;
            self.clear_at = 0;
            if self.state != ListenerState::Quiet {
                self.state = ListenerState::Playing;
            }
            self.next_identify =
                r.ends_in_ms.map_or(0, |ms| now + ms + s.listening.relisten_after_end_ms as Ms);
            return;
        }
        if self.state == ListenerState::Quiet {
            return; // the side ended while it was identifying
        }
        if r.reason == "rate_limited" || r.reason == "error" {
            self.next_identify = now + r.retry_in_ms.max(1000);
            return;
        }
        // No match: back off, then wait for the next gap.
        self.failures += 1;
        let backoff = &s.listening.retry_backoff_s;
        if self.failures as usize <= backoff.len() {
            self.next_identify = now + backoff[self.failures as usize - 1].max(1) as Ms * 1000;
        } else if self.room {
            // Probably not music (a TV, talking): stop calling Shazam and show nothing until it goes quiet.
            self.given_up = true;
            self.state = ListenerState::Quiet;
            self.next_identify = 0;
            self.art_url.clear();
            self.dimmed = false;
            self.clear_at = 0;
        } else {
            self.next_identify = 0;
        }
    }

    /// The clip couldn't be recorded or sent.
    pub fn clip_failed(&mut self, now: Ms) {
        self.busy = false;
        if self.state != ListenerState::Quiet {
            self.next_identify = now + 10_000;
        }
    }

    /// `music`: the level detector hears music. Scheduled listens (predicted
    /// track end, retries) wait for it, so a paused record isn't sent to Shazam.
    pub fn update(&mut self, now: Ms, music: bool) {
        if self.dimmed && self.clear_at != 0 && now >= self.clear_at {
            self.dimmed = false;
            self.clear_at = 0;
            self.art_url.clear();
        }
        if self.next_identify != 0 && now >= self.next_identify && self.state != ListenerState::Quiet && music {
            self.request();
        }
    }

    pub fn state(&self) -> ListenerState {
        self.state
    }
    pub fn busy(&self) -> bool {
        self.busy
    }
    pub fn art_url(&self) -> &str {
        &self.art_url
    }
    pub fn dimmed(&self) -> bool {
        self.dimmed
    }
    pub fn last(&self) -> &Recognition {
        &self.last
    }
    pub fn given_up(&self) -> bool {
        self.given_up
    }
    pub fn failures(&self) -> u32 {
        self.failures
    }
    /// None: nothing scheduled.
    pub fn next_identify_in(&self, now: Ms) -> Option<i64> {
        (self.next_identify != 0).then(|| self.next_identify as i64 - now as i64)
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Calibration {
    pub p10: f32,
    pub median: f32,
    pub p90: f32,
    pub samples: usize,
}

/// The whole display: picks the source, listens to analog ones, decides the art.
#[derive(Default)]
pub struct Device {
    pub settings: Settings,
    players: HashMap<String, Player>,
    level: Level,
    listener: Listener,
    mode: Mode,
    art: String,
    source: String,
    picture_missing_since: Ms,
    panel_off: bool,
    analog_stopped: bool,
    /// Index in settings' analog sources while in Mode::Analog; None there: the room.
    analog: Option<usize>,
    /// Which analog source, to notice a switch to another.
    analog_key: String,
    calibrating_until: Ms,
    calibration: Vec<f32>,
    calibration_ready: bool,
    brightness_override: Option<i64>,
}

/// A player that just stopped, or a picture that just vanished, keeps its art
/// this long: players flicker through idle between tracks.
const STOP_HOLD_MS: Ms = 5000;

impl Device {
    pub fn set_settings(&mut self, s: Settings) {
        self.settings = s;
    }

    /// Entities to follow in Home Assistant: digital, analog, then the idle screen's, without repeats.
    /// Sources turned off aren't followed.
    pub fn entities(&self) -> Vec<String> {
        let mut e: Vec<String> = Vec::new();
        let sources = &self.settings.sources;
        let digital = sources.digital.iter().filter(|d| d.enabled).map(|d| &d.entity);
        let analog = sources.analog.iter().filter(|a| a.enabled).map(|a| &a.entity);
        let idle = &self.settings.idle;
        let weather = (idle.show == IdleShow::Weather).then_some(&idle.weather);
        let shown = idle.entities.iter().filter(|_| idle.show == IdleShow::Entities).map(|i| &i.entity);
        for entity in digital.chain(analog).chain(weather).chain(shown) {
            if !entity.is_empty() && !e.contains(entity) {
                e.push(entity.clone());
            }
        }
        e
    }

    /// A Home Assistant state for a followed entity.
    pub fn player(&mut self, p: Player, now: Ms) {
        let stored = self.players.entry(p.entity.clone()).or_default();
        let mut state_changed = stored.state != p.state || stored.source != p.source;
        for a in &self.settings.sources.analog {
            state_changed |= a.attribute != "state"
                && p.entity == a.entity
                && stored.attribute(&a.attribute) != p.attribute(&a.attribute);
        }
        let changed = if state_changed || stored.changed == 0 { now } else { stored.changed };
        *stored = p;
        stored.changed = changed;
    }

    /// A 50 ms mic block, while `mic_wanted`.
    pub fn level(&mut self, db: f32, now: Ms) {
        if self.calibrating() {
            self.calibration.push(db);
            if now >= self.calibrating_until {
                self.calibrating_until = 0;
                self.calibration_ready = true;
            }
        }
        if self.mode != Mode::Analog {
            return;
        }
        let confirm =
            if self.room() { self.settings.listening.music_start_s as Ms * 1000 } else { Level::MUSIC_CONFIRM_MS };
        let e = self.level.feed(db, now, &self.settings, confirm);
        self.listener.level_event(e, now, &self.settings);
    }

    pub fn recognition(&mut self, r: &Recognition, now: Ms) {
        if self.mode == Mode::Analog {
            self.listener.result(r, now, &self.settings);
        }
    }

    pub fn identify_now(&mut self) {
        if self.mode == Mode::Analog {
            self.listener.identify_now();
        }
    }

    /// (mode, the player shown, the analog source)
    ///
    /// The highest source on the list that's playing: nothing else decides it. One with
    /// no cover to show, or that only just paused or stopped, keeps the screen while no
    /// other plays.
    fn choose(&self, now: Ms) -> (Mode, Option<&Player>, Option<usize>) {
        let sources = &self.settings.sources;
        let mut held: Option<(&Player, AppShow)> = None;
        let mut blank: Option<&Player> = None;
        for entity in sources.priority() {
            let Some(p) = self.players.get(entity) else { continue };
            // An analog source plays while its state says so, whatever the player says it plays.
            if let Some(i) = sources.analog.iter().position(|a| a.entity == entity && a.on(p)) {
                return (Mode::Analog, Some(p), Some(i));
            }
            if !sources.digital.iter().any(|d| d.enabled && d.entity == entity) {
                continue;
            }
            let show = self.settings.app_show(p);
            if show == AppShow::Off {
                continue;
            }
            let paused_recently =
                p.state == "paused" && now - p.changed < self.settings.display.pause_grace_s as Ms * 1000;
            let just_stopped = p.usable() && !p.playing() && p.state != "paused" && now - p.changed < STOP_HOLD_MS;
            let holds = paused_recently || just_stopped;
            // An app on the progress screen counts as playing even paused or between videos,
            // so a source below it doesn't take the screen while the next one loads.
            // Playing with no picture (an Apple TV on YouTube): the cover would be nothing,
            // so a source further down that has something to show goes first.
            if p.playing() && show == AppShow::Cover && p.picture.is_empty() {
                blank = blank.or(Some(p));
                continue;
            }
            if p.playing() || (holds && show == AppShow::Progress) {
                return (mode_for(show), Some(p), None);
            }
            if holds && held.is_none() {
                held = Some((p, show));
            }
        }
        if let Some(p) = blank {
            return (Mode::Digital, Some(p), None);
        }
        if let Some((p, show)) = held {
            return (mode_for(show), Some(p), None);
        }
        // Nothing playing that Home Assistant knows of: listen to the room. A player playing
        // in any app, even one that shows nothing (a TV on Netflix), isn't that.
        let any_playing =
            sources.digital.iter().filter(|d| d.enabled).any(|d| self.players.get(&d.entity).is_some_and(Player::playing));
        let mode = if sources.listen_for_music && !any_playing { Mode::Analog } else { Mode::Off };
        (mode, None, None)
    }

    pub fn update(&mut self, now: Ms) {
        let (next, shown, analog) = self.choose(now);
        let shown_entity = shown.map(|p| p.entity.clone()).unwrap_or_default();
        let picture = shown.map(|p| p.picture.clone()).unwrap_or_default();
        // Another analog source (the turntable stopped, the tape deck started) is a new listening session.
        let key = match analog {
            Some(i) => {
                let a = &self.settings.sources.analog[i];
                format!("{}\n{}\n{}\n{}", a.name, a.entity, a.attribute, a.value)
            }
            None if next == Mode::Analog => "room".into(),
            None => String::new(),
        };
        if self.mode == Mode::Analog && (next != Mode::Analog || key != self.analog_key) {
            self.listener = Listener::default();
            self.level = Level::default();
            self.analog_stopped = true;
        }
        let before = self.mode;
        let previous_source = std::mem::replace(&mut self.source, shown_entity);
        self.mode = next;
        self.analog = analog;
        self.analog_key = key;
        if self.mode == Mode::Analog {
            self.listener.room = self.analog.is_none();
            self.listener.update(now, self.level.music());
            self.art = self.listener.art_url().to_string();
            self.picture_missing_since = 0;
            return;
        }
        // Same player, picture gone for a moment (a track change): keep the last art.
        if picture.is_empty()
            && self.mode == Mode::Digital
            && before == Mode::Digital
            && self.source == previous_source
            && !self.art.is_empty()
        {
            if self.picture_missing_since == 0 {
                self.picture_missing_since = now;
            }
            if now - self.picture_missing_since < STOP_HOLD_MS {
                return;
            }
        }
        self.picture_missing_since = 0;
        // An app on the progress screen shows the progress screen, not its poster.
        self.art = if self.mode == Mode::Progress { String::new() } else { picture };
    }

    pub fn set_panel(&mut self, on: bool) {
        self.panel_off = !on;
    }
    pub fn panel_on(&self) -> bool {
        !self.panel_off
    }
    /// None: follow the schedule.
    pub fn set_brightness_override(&mut self, b: Option<i64>) {
        self.brightness_override = b;
    }

    pub fn brightness(&self, local_min: u32) -> i64 {
        if self.panel_off {
            return 0;
        }
        self.brightness_override.unwrap_or_else(|| self.settings.brightness_at(local_min))
    }

    /// Calibrate: collect levels for a while (mic on whatever the mode), then report.
    pub fn start_calibration(&mut self, now: Ms, span: Ms) {
        self.calibrating_until = now + span;
        self.calibration.clear();
    }
    pub fn calibrating(&self) -> bool {
        self.calibrating_until != 0
    }

    pub fn take_calibration(&mut self) -> Option<Calibration> {
        if !std::mem::take(&mut self.calibration_ready) {
            return None;
        }
        let mut v = std::mem::take(&mut self.calibration);
        if v.is_empty() {
            return Some(Calibration::default());
        }
        v.sort_by(f32::total_cmp);
        let n = v.len();
        Some(Calibration { p10: v[n / 10], median: v[n / 2], p90: v[(n * 9 / 10).min(n - 1)], samples: n })
    }

    pub fn mode(&self) -> Mode {
        self.mode
    }
    /// The art to show: a Home Assistant path ("/api/...") or a full URL; "" for none.
    pub fn art(&self) -> &str {
        &self.art
    }
    pub fn art_dimmed(&self) -> bool {
        self.mode == Mode::Analog && self.listener.dimmed()
    }
    /// Music on the analog source with no art: not recognised yet, or not at all.
    pub fn unrecognised(&self) -> bool {
        self.mode == Mode::Analog && self.art.is_empty() && self.listener.state() != ListenerState::Quiet
    }
    pub fn mic_wanted(&self) -> bool {
        self.mode == Mode::Analog || self.calibrating()
    }
    pub fn take_clip_request(&mut self) -> bool {
        self.listener.take_clip_request()
    }
    pub fn clip_failed(&mut self, now: Ms) {
        self.listener.clip_failed(now)
    }
    /// True once after an analog source stops (its listening session ends).
    pub fn take_analog_stopped(&mut self) -> bool {
        std::mem::take(&mut self.analog_stopped)
    }
    pub fn listener(&self) -> &Listener {
        &self.listener
    }
    pub fn levels(&self) -> &Level {
        &self.level
    }
    /// The analog source being listened to, if any (none when listening to the room).
    pub fn analog_source(&self) -> Option<&AnalogSource> {
        if self.mode != Mode::Analog {
            return None;
        }
        self.analog.and_then(|i| self.settings.sources.analog.get(i))
    }
    /// Listening to the room for music, with nothing else playing.
    pub fn room(&self) -> bool {
        self.mode == Mode::Analog && self.analog.is_none()
    }
    /// The player being shown, if any.
    pub fn source(&self) -> Option<&Player> {
        self.players.get(&self.source)
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::settings::DigitalSource;

    pub(crate) fn player(entity: &str, state: &str, source: &str, picture: &str) -> Player {
        Player { entity: entity.into(), state: state.into(), source: source.into(), picture: picture.into(), ..Default::default() }
    }

    fn digital(entity: &str, apps: &[&str]) -> DigitalSource {
        let apps = apps.iter().map(|a| a.to_string()).collect();
        DigitalSource { entity: entity.into(), apps, progress: vec![], off: vec![], exclude: false, other: None, enabled: true }
    }

    fn analog(name: &str, entity: &str, attribute: &str, value: &str) -> AnalogSource {
        AnalogSource { name: name.into(), entity: entity.into(), attribute: attribute.into(), value: value.into(), enabled: true }
    }

    /// The setup these tests grew up on: a WiiM Ultra (digital) that switches to its
    /// phono input for the turntable (analog), and an Apple TV (digital).
    pub(crate) fn house_settings() -> Settings {
        let mut s = Settings::default();
        s.sources.digital = vec![digital("media_player.wiim_ultra_2", &[]), digital("media_player.apple_tv", &[])];
        s.sources.analog = vec![analog("Turntable", "media_player.wiim_ultra_2", "source", "Phono In")];
        s
    }

    fn house() -> Device {
        let mut d = Device::default();
        d.set_settings(house_settings());
        d
    }

    /// Feed a level for a span of time in 50 ms blocks.
    fn feed(d: &mut Device, db: f32, now: &mut Ms, span: Ms) {
        let end = *now + span;
        while *now < end {
            d.level(db, *now);
            d.update(*now);
            *now += 50;
        }
    }

    fn analog_device(now: Ms) -> Device {
        let mut d = house();
        d.player(player("media_player.wiim_ultra_2", "idle", "Phono In", ""), now);
        d.update(now);
        d
    }

    fn hit(art: &str, ends_in_ms: u64) -> Recognition {
        Recognition { matched: true, reason: String::new(), art_url: art.into(), ends_in_ms: Some(ends_in_ms), ..Default::default() }
    }

    fn miss(reason: &str) -> Recognition {
        Recognition { reason: reason.into(), ..Default::default() }
    }

    #[test]
    fn night_brightness_spans_midnight() {
        let s = Settings::default();
        assert_eq!(s.brightness_at(12 * 60), s.display.brightness_day);
        assert_eq!(s.brightness_at(23 * 60), s.display.brightness_night);
        assert_eq!(s.brightness_at(3 * 60), s.display.brightness_night);
        assert_eq!(s.brightness_at(7 * 60), s.display.brightness_day);
    }

    #[test]
    fn no_schedule_keeps_day_brightness() {
        let mut s = Settings::default();
        s.display.brightness_schedule = false;
        assert_eq!(s.brightness_at(23 * 60), s.display.brightness_day);
    }

    #[test]
    fn digital_picks_the_playing_player() {
        let mut d = house();
        d.player(player("media_player.apple_tv", "playing", "", "/tv.jpg"), 1000);
        d.update(1000);
        assert_eq!((d.mode(), d.art()), (Mode::Digital, "/tv.jpg"));
        d.player(player("media_player.wiim_ultra_2", "playing", "Network", "/wiim.jpg"), 2000);
        d.update(2000);
        assert_eq!(d.art(), "/wiim.jpg"); // listed first
        d.player(player("media_player.wiim_ultra_2", "idle", "Network", ""), 3000);
        d.update(3000);
        assert_eq!(d.art(), "/tv.jpg");
    }

    #[test]
    fn apps_not_on_the_list_are_ignored() {
        let mut s = Settings::default();
        s.sources.digital =
            vec![digital("media_player.wiim_ultra_2", &[]), digital("media_player.apple_tv", &["Music", "com.spotify.client"])];
        let mut d = Device::default();
        d.set_settings(s.clone());
        let mut tv = player("media_player.apple_tv", "playing", "", "/movie.jpg");
        tv.app_name = "Netflix".into();
        tv.app_id = "com.netflix.Netflix".into();
        d.player(tv.clone(), 1000);
        d.update(1000);
        assert_eq!((d.mode(), d.art()), (Mode::Off, ""));
        tv.app_name = "music".into(); // any case
        tv.app_id = "com.apple.TVMusic".into();
        tv.picture = "/album.jpg".into();
        d.player(tv.clone(), 2000);
        d.update(2000);
        assert_eq!((d.mode(), d.art()), (Mode::Digital, "/album.jpg"));
        tv.app_name = "Spotify".into(); // matched by app_id
        tv.app_id = "com.spotify.client".into();
        d.player(tv.clone(), 3000);
        d.update(3000);
        assert_eq!(d.mode(), Mode::Digital);
        // The list is only for its own player: the WiiM plays whatever its app.
        let mut wiim = player("media_player.wiim_ultra_2", "playing", "Network", "/wiim.jpg");
        wiim.app_name = "Netflix".into();
        tv.app_name = "Netflix".into();
        tv.app_id.clear();
        d.player(tv, 4000);
        d.player(wiim, 4000);
        d.update(4000);
        assert_eq!(d.art(), "/wiim.jpg");
        // An empty list lets every app through.
        s.sources.digital[1].apps.clear();
        d.set_settings(s);
        d.player(player("media_player.wiim_ultra_2", "idle", "Network", ""), 5000);
        d.update(5000);
        assert_eq!(d.mode(), Mode::Digital);
        assert_eq!(d.source().unwrap().entity, "media_player.apple_tv");
    }

    #[test]
    fn video_apps_show_the_progress_screen() {
        let mut s = Settings::default();
        s.sources.digital = vec![DigitalSource { progress: vec!["Netflix".into()], ..digital("media_player.apple_tv", &["Music"]) }];
        let mut d = Device::default();
        d.set_settings(s.clone());
        let mut tv = player("media_player.apple_tv", "playing", "", "/poster.jpg");
        tv.app_name = "netflix".into(); // any case
        d.player(tv.clone(), 1000);
        d.update(1000);
        assert_eq!((d.mode(), d.art()), (Mode::Progress, "")); // no poster
        assert_eq!(d.source().unwrap().entity, "media_player.apple_tv");
        // Older settings: an app on no list shows nothing when there's an art list...
        tv.app_name = "YouTube".into();
        d.player(tv.clone(), 2000);
        d.update(2000);
        assert_eq!(d.mode(), Mode::Off);
        // ...and every other app can be sent to the progress screen, or to its cover.
        s.sources.digital[0].other = Some(AppShow::Progress);
        d.set_settings(s.clone());
        d.update(2000);
        assert_eq!(d.mode(), Mode::Progress);
        s.sources.digital[0].other = Some(AppShow::Cover);
        s.sources.digital[0].off = vec!["YouTube".into()];
        d.set_settings(s.clone());
        d.update(2000);
        assert_eq!(d.mode(), Mode::Off);
        tv.app_name = "Plex".into();
        d.player(tv, 3000);
        d.update(3000);
        assert_eq!((d.mode(), d.art()), (Mode::Digital, "/poster.jpg"));
    }

    #[test]
    fn the_progress_screen_stays_between_videos() {
        // The WiiM plays the TV's sound on HDMI, "Unknown" with a picture, below the Apple TV on the list.
        let mut s = Settings::default();
        s.sources.digital = vec![
            DigitalSource { progress: vec!["YouTube".into()], ..digital("media_player.apple_tv", &[]) },
            DigitalSource { exclude: true, ..digital("media_player.wiim", &["Phono"]) },
        ];
        let mut d = Device::default();
        d.set_settings(s);
        let mut wiim = player("media_player.wiim", "playing", "HDMI", "/hdmi.jpg");
        wiim.title = "Unknown".into();
        d.player(wiim, 1000);
        let mut tv = player("media_player.apple_tv", "playing", "", "/thumb.jpg");
        tv.app_name = "YouTube".into();
        tv.title = "A video".into();
        d.player(tv.clone(), 1000);
        d.update(1000);
        assert_eq!(d.mode(), Mode::Progress);
        // Paused, then playing the next one before its title comes: still the progress screen.
        for (state, title, at) in [("paused", "A video", 2000), ("idle", "", 3000), ("playing", "", 4000)] {
            tv.state = state.into();
            tv.title = title.into();
            d.player(tv.clone(), at);
            d.update(at);
            assert_eq!((d.mode(), d.source().unwrap().entity.as_str()), (Mode::Progress, "media_player.apple_tv"), "{state}");
        }
    }

    #[test]
    fn older_app_lists_read_as_before() {
        let tv = |app: &str| Player { entity: "media_player.tv".into(), app_name: app.into(), ..Default::default() };
        let mut s = Settings::default();
        assert_eq!(s.app_show(&tv("Netflix")), AppShow::Cover); // not a source: art
        s.sources.digital = vec![digital("media_player.tv", &[])];
        assert_eq!(s.app_show(&tv("Netflix")), AppShow::Cover);
        s.sources.digital = vec![digital("media_player.tv", &["Music"])];
        assert_eq!((s.app_show(&tv("Music")), s.app_show(&tv("Netflix"))), (AppShow::Cover, AppShow::Off));
        s.sources.digital[0].exclude = true;
        assert_eq!((s.app_show(&tv("Music")), s.app_show(&tv("Netflix"))), (AppShow::Off, AppShow::Cover));
    }

    #[test]
    fn progress_position_runs_while_playing() {
        let mut p = player("media_player.apple_tv", "playing", "", "");
        assert_eq!(p.position_now(1000.0), None);
        (p.position, p.position_at, p.duration) = (Some(600.0), Some(1000.0), Some(3600.0));
        assert_eq!(p.position_now(1090.0), Some(690.0));
        assert_eq!(p.position_now(9000.0), Some(3600.0)); // never past the end
        p.state = "paused".into();
        assert_eq!(p.position_now(1090.0), Some(600.0));
        p.state = "playing".into();
        p.duration = None;
        assert_eq!(p.position_now(1010.0), Some(610.0));
    }

    #[test]
    fn excluded_apps_are_ignored() {
        let mut s = Settings::default();
        s.sources.digital = vec![DigitalSource { exclude: true, ..digital("media_player.wiim_ultra", &["HDMI"]) }];
        let mut d = Device::default();
        d.set_settings(s);
        d.player(player("media_player.wiim_ultra", "playing", "hdmi", "/tv.jpg"), 1000); // any case
        d.update(1000);
        assert_eq!((d.mode(), d.art()), (Mode::Off, ""));
        d.player(player("media_player.wiim_ultra", "playing", "Spotify", "/song.jpg"), 2000);
        d.update(2000);
        assert_eq!((d.mode(), d.art()), (Mode::Digital, "/song.jpg"));
    }

    #[test]
    fn paused_keeps_art_for_the_grace() {
        let mut d = house();
        d.player(player("media_player.apple_tv", "playing", "", "/tv.jpg"), 0);
        d.player(player("media_player.apple_tv", "paused", "", "/tv.jpg"), 1000);
        d.update(20000);
        assert_eq!(d.mode(), Mode::Digital);
        d.update(31001);
        assert_eq!((d.mode(), d.art()), (Mode::Off, ""));
    }

    #[test]
    fn analog_source_means_listening() {
        let mut d = house();
        d.player(player("media_player.apple_tv", "playing", "", "/tv.jpg"), 0);
        d.player(player("media_player.wiim_ultra_2", "idle", "Phono In", ""), 0);
        d.update(0);
        assert!(d.mode() == Mode::Analog && d.mic_wanted() && d.art().is_empty());
        d.player(player("media_player.wiim_ultra_2", "idle", "Network", ""), 5000);
        d.update(5000);
        assert_eq!(d.mode(), Mode::Digital);
        assert!(d.take_analog_stopped() && !d.take_analog_stopped());
    }

    #[test]
    fn music_triggers_identify_then_art() {
        let mut now = 0;
        let mut d = analog_device(now);
        feed(&mut d, -70.0, &mut now, 2000);
        assert!(!d.take_clip_request());
        feed(&mut d, -30.0, &mut now, 1100);
        assert!(d.take_clip_request());
        assert!(!d.take_clip_request()); // once
        assert!(d.listener().busy());
        feed(&mut d, -30.0, &mut now, 10000);
        assert!(!d.take_clip_request()); // busy while the clip is out
        d.recognition(&hit("https://art/1.jpg", 120000), now);
        d.update(now);
        assert_eq!(d.art(), "https://art/1.jpg");
        assert_eq!(d.listener().state(), ListenerState::Playing);
        // Listens again 1 s after the predicted end.
        feed(&mut d, -30.0, &mut now, 120000);
        assert!(!d.take_clip_request());
        feed(&mut d, -30.0, &mut now, 1100);
        assert!(d.take_clip_request());
    }

    #[test]
    fn gap_between_tracks_identifies_early() {
        let mut now = 0;
        let mut d = analog_device(now);
        feed(&mut d, -30.0, &mut now, 1100);
        assert!(d.take_clip_request());
        d.recognition(&hit("a", 200000), now);
        feed(&mut d, -30.0, &mut now, 30000);
        feed(&mut d, -65.0, &mut now, 2000); // a 2 s gap
        assert!(!d.take_clip_request());
        feed(&mut d, -30.0, &mut now, 1100);
        assert!(d.take_clip_request());
    }

    #[test]
    fn short_dip_is_not_a_gap() {
        let mut now = 0;
        let mut d = analog_device(now);
        feed(&mut d, -30.0, &mut now, 1100);
        assert!(d.take_clip_request());
        d.recognition(&hit("a", 200000), now);
        feed(&mut d, -65.0, &mut now, 400); // under min_gap_ms
        feed(&mut d, -30.0, &mut now, 3000);
        assert!(!d.take_clip_request());
    }

    #[test]
    fn no_match_backs_off_then_waits_for_a_gap() {
        let mut now = 0;
        let mut d = analog_device(now);
        feed(&mut d, -30.0, &mut now, 1100);
        assert!(d.take_clip_request());
        for backoff in [10, 20, 40] {
            d.recognition(&miss("no_match"), now);
            feed(&mut d, -30.0, &mut now, backoff * 1000 - 100);
            assert!(!d.take_clip_request());
            feed(&mut d, -30.0, &mut now, 200);
            assert!(d.take_clip_request());
        }
        d.recognition(&miss("no_match"), now);
        feed(&mut d, -30.0, &mut now, 300000);
        assert!(!d.take_clip_request()); // out of retries
        assert_eq!(d.listener().state(), ListenerState::Listening);
    }

    #[test]
    fn rate_limit_waits_as_told() {
        let mut now = 0;
        let mut d = analog_device(now);
        feed(&mut d, -30.0, &mut now, 1100);
        assert!(d.take_clip_request());
        d.recognition(&Recognition { reason: "rate_limited".into(), retry_in_ms: 60000, ..Default::default() }, now);
        feed(&mut d, -30.0, &mut now, 59000);
        assert!(!d.take_clip_request());
        feed(&mut d, -30.0, &mut now, 1100);
        assert!(d.take_clip_request());
        assert_eq!(d.listener().failures(), 0);
    }

    #[test]
    fn side_end_dims_then_clears() {
        let mut now = 0;
        let mut d = analog_device(now);
        feed(&mut d, -30.0, &mut now, 1100);
        assert!(d.take_clip_request());
        d.recognition(&hit("lp", 200000), now);
        feed(&mut d, -30.0, &mut now, 5000);
        feed(&mut d, -70.0, &mut now, 31000);
        assert_eq!(d.listener().state(), ListenerState::Quiet);
        assert!(d.art() == "lp" && d.art_dimmed());
        feed(&mut d, -70.0, &mut now, 120000);
        assert!(d.art().is_empty() && !d.art_dimmed());
    }

    #[test]
    fn flipping_the_record_keeps_the_album_until_the_new_match() {
        let mut now = 0;
        let mut d = analog_device(now);
        feed(&mut d, -30.0, &mut now, 1100);
        assert!(d.take_clip_request());
        d.recognition(&hit("lp", 200000), now);
        feed(&mut d, -70.0, &mut now, 40000); // side ends, dimmed
        feed(&mut d, -30.0, &mut now, 1100); // side B starts
        assert!(d.take_clip_request());
        assert_eq!(d.art(), "lp");
        d.recognition(&hit("lp-b", 200000), now);
        d.update(now);
        assert!(d.art() == "lp-b" && !d.art_dimmed());
    }

    #[test]
    fn digital_next_track_never_blinks() {
        let mut d = house();
        d.player(player("media_player.wiim_ultra_2", "playing", "Network", "/a.jpg"), 0);
        d.update(0);
        assert_eq!(d.art(), "/a.jpg");
        // Between tracks the player flickers: buffering without a picture, then idle, then the next song.
        d.player(player("media_player.wiim_ultra_2", "buffering", "Network", ""), 200000);
        d.update(201000);
        assert_eq!((d.mode(), d.art()), (Mode::Digital, "/a.jpg"));
        d.player(player("media_player.wiim_ultra_2", "idle", "Network", ""), 202000);
        d.update(203000);
        assert_eq!((d.mode(), d.art()), (Mode::Digital, "/a.jpg"));
        d.player(player("media_player.wiim_ultra_2", "playing", "Network", "/b.jpg"), 204000);
        d.update(204000);
        assert_eq!(d.art(), "/b.jpg");
    }

    #[test]
    fn an_input_turned_off_gives_way() {
        // The TV's sound on the WiiM plays "Unknown" while the Apple TV names the song.
        let mut d = house();
        let tv = Player { title: "I'm On One".into(), ..player("media_player.apple_tv", "playing", "", "/tv.jpg") };
        d.player(tv, 0);
        let wiim = Player { title: "Unknown".into(), ..player("media_player.wiim_ultra_2", "playing", "HDMI", "/wiim.jpg") };
        d.player(wiim, 5000);
        d.update(5000);
        assert_eq!((d.mode(), d.art()), (Mode::Digital, "/wiim.jpg")); // higher on the list: only the order decides
        let mut s = house_settings();
        s.sources.digital[0].off = vec!["HDMI".into()];
        d.set_settings(s);
        d.update(6000);
        assert_eq!((d.mode(), d.art()), (Mode::Digital, "/tv.jpg"));
    }

    #[test]
    fn a_player_with_no_cover_gives_way() {
        let mut s = Settings::default();
        s.sources.digital = vec![digital("media_player.apple_tv", &[]), digital("media_player.youtube", &[])];
        let mut d = Device::default();
        d.set_settings(s);
        // Higher on the list and playing, but with no picture: alone, it has the screen.
        d.player(player("media_player.apple_tv", "playing", "", ""), 0);
        d.update(0);
        assert_eq!((d.mode(), d.source().unwrap().entity.as_str()), (Mode::Digital, "media_player.apple_tv"));
        d.player(player("media_player.youtube", "playing", "", "/thumb.jpg"), 1000);
        d.update(1000);
        assert_eq!((d.mode(), d.art()), (Mode::Digital, "/thumb.jpg"));
        // With a picture, the order decides again.
        d.player(player("media_player.apple_tv", "playing", "", "/tv.jpg"), 2000);
        d.update(2000);
        assert_eq!(d.art(), "/tv.jpg");
    }

    #[test]
    fn the_order_puts_a_player_above_an_analog_source() {
        let mut s = house_settings();
        let mut d = Device::default();
        d.set_settings(s.clone());
        d.player(player("media_player.apple_tv", "playing", "", "/tv.jpg"), 0);
        d.player(player("media_player.wiim_ultra_2", "idle", "Phono In", ""), 0);
        d.update(0);
        assert_eq!(d.mode(), Mode::Analog); // older settings, no order: analog is heard first
        s.sources.order = vec!["media_player.apple_tv".into(), "media_player.wiim_ultra_2".into()];
        d.set_settings(s.clone());
        d.update(1000);
        assert_eq!((d.mode(), d.art()), (Mode::Digital, "/tv.jpg"));
        // A source the order doesn't name comes after those it does; one that's gone is skipped.
        s.sources.order = vec!["media_player.gone".into(), "media_player.apple_tv".into()];
        assert_eq!(s.sources.priority(), ["media_player.apple_tv", "media_player.wiim_ultra_2"]);
    }

    #[test]
    fn wiim_switching_to_tv_keeps_the_apple_tv_art() {
        let mut s = house_settings();
        s.sources.digital[0].off = vec!["TV".into()];
        let mut d = Device::default();
        d.set_settings(s);
        d.player(player("media_player.apple_tv", "playing", "", "/tv.jpg"), 0);
        d.player(player("media_player.wiim_ultra_2", "idle", "Phono In", ""), 0);
        d.update(0);
        assert_eq!(d.mode(), Mode::Analog);
        d.player(player("media_player.wiim_ultra_2", "playing", "TV", ""), 5000);
        d.update(5000);
        assert_eq!((d.mode(), d.art()), (Mode::Digital, "/tv.jpg"));
        assert!(d.take_analog_stopped());
        d.player(player("media_player.wiim_ultra_2", "playing", "Phono In", ""), 60000);
        d.update(60000);
        assert!(d.mode() == Mode::Analog && d.mic_wanted());
    }

    #[test]
    fn digital_queue_end_turns_off() {
        let mut d = house();
        d.player(player("media_player.apple_tv", "playing", "", "/a.jpg"), 0);
        d.player(player("media_player.apple_tv", "idle", "", ""), 10000);
        d.update(14000);
        assert_eq!(d.mode(), Mode::Digital); // could be a track change
        d.update(15001);
        assert_eq!((d.mode(), d.art()), (Mode::Off, ""));
    }

    #[test]
    fn digital_skip_mid_song() {
        let mut d = house();
        d.player(player("media_player.apple_tv", "playing", "", "/a.jpg"), 0);
        d.update(0);
        d.player(player("media_player.apple_tv", "playing", "", "/b.jpg"), 60000);
        d.update(60000);
        assert_eq!(d.art(), "/b.jpg");
        d.player(player("media_player.apple_tv", "playing", "", "/b.jpg"), 70000);
        d.update(70000);
        assert_eq!(d.art(), "/b.jpg");
        // A picture that stays missing does go.
        d.player(player("media_player.apple_tv", "playing", "", ""), 80000);
        d.update(80000);
        d.update(84000);
        assert_eq!(d.art(), "/b.jpg");
        d.update(85001);
        assert_eq!(d.art(), "");
    }

    #[test]
    fn digital_pause_and_resume() {
        let mut d = house();
        d.player(player("media_player.apple_tv", "playing", "", "/a.jpg"), 0);
        d.player(player("media_player.apple_tv", "paused", "", "/a.jpg"), 10000);
        d.update(35000);
        assert_eq!(d.art(), "/a.jpg"); // within the 30 s grace
        d.player(player("media_player.apple_tv", "playing", "", "/a.jpg"), 36000);
        d.update(36000);
        assert_eq!(d.art(), "/a.jpg");
        d.player(player("media_player.apple_tv", "paused", "", "/a.jpg"), 50000);
        d.update(80001);
        assert_eq!(d.mode(), Mode::Off); // a long pause turns the panel off
        d.player(player("media_player.apple_tv", "playing", "", "/a.jpg"), 90000);
        d.update(90000);
        assert_eq!((d.mode(), d.art()), (Mode::Digital, "/a.jpg"));
    }

    #[test]
    fn analog_song_to_song_with_a_gap() {
        let mut now = 0;
        let mut d = analog_device(now);
        feed(&mut d, -30.0, &mut now, 1100);
        assert!(d.take_clip_request());
        d.recognition(&hit("lp", 60000), now); // 60 s left in song 1
        feed(&mut d, -30.0, &mut now, 58000);
        feed(&mut d, -65.0, &mut now, 2000); // the groove between songs
        feed(&mut d, -30.0, &mut now, 1100);
        assert!(d.take_clip_request()); // song 2, identified by the gap
        d.recognition(&hit("lp", 240000), now);
        feed(&mut d, -30.0, &mut now, 60000);
        assert!(!d.take_clip_request()); // song 1's predicted end no longer fires
        assert_eq!(d.art(), "lp");
    }

    #[test]
    fn analog_song_to_song_without_a_gap() {
        let mut now = 0;
        let mut d = analog_device(now);
        feed(&mut d, -30.0, &mut now, 1100);
        assert!(d.take_clip_request());
        d.recognition(&hit("lp", 60000), now);
        feed(&mut d, -30.0, &mut now, 60000);
        assert!(!d.take_clip_request());
        feed(&mut d, -30.0, &mut now, 2000); // segue: the predicted end (+1 s) listens again
        assert!(d.take_clip_request());
    }

    #[test]
    fn analog_needle_moved_mid_song() {
        let mut now = 0;
        let mut d = analog_device(now);
        feed(&mut d, -30.0, &mut now, 1100);
        assert!(d.take_clip_request());
        d.recognition(&hit("lp", 200000), now);
        feed(&mut d, -30.0, &mut now, 60000);
        feed(&mut d, -75.0, &mut now, 3000); // needle up, moved, down
        feed(&mut d, -30.0, &mut now, 1100);
        assert!(d.take_clip_request());
        assert!(d.art() == "lp" && !d.art_dimmed()); // the album stays while it listens
    }

    #[test]
    fn analog_short_pause_waits_for_music() {
        let mut now = 0;
        let mut d = analog_device(now);
        feed(&mut d, -30.0, &mut now, 1100);
        assert!(d.take_clip_request());
        d.recognition(&hit("lp", 20000), now);
        feed(&mut d, -30.0, &mut now, 10000);
        feed(&mut d, -75.0, &mut now, 20000); // paused through the predicted end
        assert!(!d.take_clip_request()); // no clip of silence
        assert!(d.art() == "lp" && !d.art_dimmed());
        feed(&mut d, -30.0, &mut now, 1100); // resumed
        assert!(d.take_clip_request());
    }

    #[test]
    fn analog_long_pause_dims_then_resumes() {
        let mut now = 0;
        let mut d = analog_device(now);
        feed(&mut d, -30.0, &mut now, 1100);
        assert!(d.take_clip_request());
        d.recognition(&hit("lp", 200000), now);
        feed(&mut d, -75.0, &mut now, 60000); // a minute's pause looks like the side ended
        assert!(d.art() == "lp" && d.art_dimmed());
        feed(&mut d, -30.0, &mut now, 1100);
        assert!(d.take_clip_request());
        d.recognition(&hit("lp", 140000), now);
        d.update(now);
        assert!(d.art() == "lp" && !d.art_dimmed());
    }

    #[test]
    fn analog_retry_waits_through_silence() {
        let mut now = 0;
        let mut d = analog_device(now);
        feed(&mut d, -30.0, &mut now, 1100);
        assert!(d.take_clip_request());
        d.recognition(&miss("no_match"), now); // retry in 10 s...
        feed(&mut d, -75.0, &mut now, 15000); // ...but it's quiet
        assert!(!d.take_clip_request());
        feed(&mut d, -30.0, &mut now, 1100);
        assert!(d.take_clip_request());
    }

    #[test]
    fn identify_now_from_the_portal() {
        let mut d = analog_device(0);
        d.identify_now();
        assert!(d.take_clip_request());
    }

    #[test]
    fn no_sources_means_off_and_follows_nothing() {
        let mut d = Device::default();
        assert!(d.entities().is_empty());
        d.player(player("media_player.apple_tv", "playing", "", "/tv.jpg"), 0);
        d.update(0);
        assert!(d.mode() == Mode::Off && d.art().is_empty() && !d.mic_wanted());
    }

    #[test]
    fn entities_are_digital_then_analog_without_repeats() {
        let mut d = house();
        assert_eq!(d.entities(), ["media_player.wiim_ultra_2", "media_player.apple_tv"]);
        let mut s = Settings::default();
        s.sources.digital = vec![digital("media_player.sonos", &[])];
        s.sources.analog = vec![analog("Turntable", "input_boolean.turntable", "state", "on")];
        d.set_settings(s.clone());
        assert_eq!(d.entities(), ["media_player.sonos", "input_boolean.turntable"]);
        s.sources.analog.clear();
        d.set_settings(s);
        assert_eq!(d.entities(), ["media_player.sonos"]);
    }

    #[test]
    fn analog_on_a_switch_state() {
        let mut d = Device::default();
        let mut s = Settings::default();
        s.sources.digital = vec![digital("media_player.sonos", &[])];
        s.sources.analog = vec![analog("Turntable", "switch.turntable", "state", "on")];
        d.set_settings(s.clone());
        d.player(player("media_player.sonos", "playing", "", "/sonos.jpg"), 0);
        d.update(0);
        assert_eq!(d.mode(), Mode::Digital);
        d.player(player("switch.turntable", "on", "", ""), 1000);
        d.update(1000);
        assert!(d.mode() == Mode::Analog && d.mic_wanted());
        d.player(player("switch.turntable", "off", "", ""), 2000);
        d.update(2000);
        assert!(d.mode() == Mode::Digital && d.take_analog_stopped());
        // "state" compares even states that aren't usable: "off" can mean playing if asked.
        s.sources.analog = vec![analog("Turntable", "switch.turntable", "state", "off")];
        d.set_settings(s);
        d.update(3000);
        assert_eq!(d.mode(), Mode::Analog);
    }

    #[test]
    fn analog_on_any_attribute() {
        let mut d = Device::default();
        let mut s = Settings::default();
        s.sources.digital = vec![digital("media_player.receiver", &[])];
        s.sources.analog = vec![analog("Turntable", "media_player.receiver", "sound_mode", "PHONO")];
        d.set_settings(s);
        let mut rx = player("media_player.receiver", "on", "", "/rx.jpg");
        rx.attributes.insert("sound_mode".into(), "MOVIE".into());
        d.player(rx.clone(), 0);
        d.update(0);
        assert_ne!(d.mode(), Mode::Analog);
        rx.attributes.insert("sound_mode".into(), "PHONO".into());
        d.player(rx.clone(), 1000);
        d.update(1000);
        assert_eq!(d.mode(), Mode::Analog);
        rx.attributes.insert("sound_mode".into(), "phono".into()); // values match exactly
        d.player(rx.clone(), 2000);
        d.update(2000);
        assert_ne!(d.mode(), Mode::Analog);
        // An attribute compare needs a usable player.
        rx.attributes.insert("sound_mode".into(), "PHONO".into());
        rx.state = "unavailable".into();
        d.player(rx, 3000);
        d.update(3000);
        assert_eq!(d.mode(), Mode::Off);
    }

    #[test]
    fn analog_needs_an_entity_and_a_value() {
        let mut p = player("media_player.wiim_ultra_2", "idle", "", "");
        assert!(!analog("Turntable", "", "source", "").on(&p));
        assert!(!analog("Turntable", "media_player.wiim_ultra_2", "source", "").on(&p));
        p.source = "Phono In".into();
        assert!(!analog("Turntable", "media_player.other", "source", "Phono In").on(&p));
        assert!(analog("", "media_player.wiim_ultra_2", "source", "Phono In").on(&p)); // a name is optional
    }

    #[test]
    fn app_list_on_any_player_matches_its_input() {
        let mut d = Device::default();
        let mut s = Settings::default();
        s.sources.digital = vec![digital("media_player.living_room", &["Spotify"])];
        d.set_settings(s);
        d.player(player("media_player.living_room", "playing", "TV", "/tv-input.jpg"), 0);
        d.update(0);
        assert_eq!(d.mode(), Mode::Off);
        d.player(player("media_player.living_room", "playing", "spotify", "/song.jpg"), 1000);
        d.update(1000);
        assert_eq!((d.mode(), d.art()), (Mode::Digital, "/song.jpg"));
    }

    #[test]
    fn player_order_picks_the_winner() {
        let mut d = Device::default();
        let mut s = Settings::default();
        s.sources.digital = vec![digital("media_player.b", &[]), digital("media_player.a", &[])];
        d.set_settings(s);
        d.player(player("media_player.b", "playing", "", "/b.jpg"), 0);
        d.player(player("media_player.a", "playing", "", "/a.jpg"), 1000);
        d.update(1000);
        assert_eq!(d.art(), "/b.jpg"); // higher on the list, though a started last
        d.player(player("media_player.b", "paused", "", "/b.jpg"), 2000);
        d.update(2000);
        assert_eq!(d.art(), "/a.jpg"); // playing beats paused
        d.player(player("media_player.b", "playing", "", "/b.jpg"), 3000);
        d.update(3000);
        assert_eq!(d.art(), "/b.jpg");
    }

    #[test]
    fn sources_turned_off_are_skipped() {
        let mut d = Device::default();
        let mut s = Settings::default();
        s.sources.digital = vec![DigitalSource { enabled: false, ..digital("media_player.tv", &[]) }, digital("media_player.wiim", &[])];
        s.sources.analog = vec![AnalogSource { enabled: false, ..analog("Turntable", "switch.turntable", "state", "on") }];
        s.sources.listen_for_music = true;
        d.set_settings(s);
        assert_eq!(d.entities(), ["media_player.wiim"]);
        d.player(player("switch.turntable", "on", "", ""), 0);
        d.player(player("media_player.tv", "playing", "", "/tv.jpg"), 0);
        d.update(0);
        // Neither the turntable nor the TV counts: the room is listened to, as with nothing playing.
        assert!(d.room());
        d.player(player("media_player.wiim", "playing", "", "/wiim.jpg"), 1000);
        d.update(1000);
        assert_eq!((d.mode(), d.art()), (Mode::Digital, "/wiim.jpg"));
    }

    #[test]
    fn several_analog_sources_first_playing_wins() {
        let mut d = Device::default();
        let mut s = Settings::default();
        s.sources.digital = vec![digital("media_player.wiim", &[])];
        s.sources.analog =
            vec![analog("Turntable", "media_player.wiim", "source", "Phono In"), analog("Tape deck", "switch.tape_deck", "state", "on")];
        d.set_settings(s);
        assert_eq!(d.entities(), ["media_player.wiim", "switch.tape_deck"]);
        d.player(player("switch.tape_deck", "on", "", ""), 0);
        d.update(0);
        assert_eq!(d.analog_source().map(|a| a.name.as_str()), Some("Tape deck"));
        assert!(!d.take_analog_stopped());
        // Both on: the one listed first wins, and the change is a new listening session.
        d.player(player("media_player.wiim", "idle", "Phono In", ""), 1000);
        d.update(1000);
        assert_eq!(d.analog_source().map(|a| a.name.as_str()), Some("Turntable"));
        assert!(d.take_analog_stopped());
        d.player(player("media_player.wiim", "idle", "Network", ""), 2000);
        d.update(2000);
        assert_eq!(d.analog_source().map(|a| a.name.as_str()), Some("Tape deck"));
        assert!(d.take_analog_stopped());
        d.update(3000);
        assert!(!d.take_analog_stopped()); // staying on one source is one session
        d.player(player("switch.tape_deck", "off", "", ""), 4000);
        d.update(4000);
        assert!(d.mode() != Mode::Analog && d.analog_source().is_none() && d.take_analog_stopped());
    }

    #[test]
    fn switching_analog_sources_forgets_the_album() {
        let mut d = Device::default();
        let mut s = Settings::default();
        s.sources.analog =
            vec![analog("Turntable", "switch.turntable", "state", "on"), analog("Tape deck", "switch.tape_deck", "state", "on")];
        d.set_settings(s);
        let mut now = 0;
        d.player(player("switch.tape_deck", "on", "", ""), now);
        d.update(now);
        feed(&mut d, -30.0, &mut now, 1500);
        assert!(d.take_clip_request());
        d.recognition(&Recognition { matched: true, art_url: "https://art/tape.jpg".into(), ..Default::default() }, now);
        d.update(now);
        assert_eq!(d.art(), "https://art/tape.jpg");
        now += 1000;
        d.player(player("switch.turntable", "on", "", ""), now);
        d.update(now);
        assert_eq!(d.analog_source().map(|a| a.name.as_str()), Some("Turntable"));
        assert_eq!(d.art(), ""); // the tape's album isn't the record's
        assert_eq!(d.listener().state(), ListenerState::Quiet);
    }

    fn room(digital: Vec<DigitalSource>) -> Device {
        let mut s = Settings::default();
        s.sources.digital = digital;
        s.sources.listen_for_music = true;
        s.listening.music_start_s = 5;
        s.listening.retry_backoff_s = vec![10];
        let mut d = Device::default();
        d.set_settings(s);
        d
    }

    #[test]
    fn room_next_track_after_a_gap_confirms_fast() {
        let mut d = room(vec![]);
        let mut now = 0;
        d.update(now);
        feed(&mut d, -30.0, &mut now, 6000);
        assert!(d.take_clip_request()); // from silence: the room's 5 s
        d.recognition(&hit("a", 600_000), now);
        feed(&mut d, -30.0, &mut now, 30000);
        feed(&mut d, -70.0, &mut now, 2000); // the gap between tracks
        feed(&mut d, -30.0, &mut now, 1100);
        assert!(d.take_clip_request()); // the next track: 1 s
    }

    #[test]
    fn room_music_with_quiet_moments_still_counts() {
        // Music above -45 dBFS half the time and between the thresholds otherwise,
        // never a gap: it used to restart the count on every dip and never confirm.
        let mut d = room(vec![]);
        let mut now = 0;
        d.update(now);
        for _ in 0..12 {
            feed(&mut d, -40.0, &mut now, 300);
            feed(&mut d, -50.0, &mut now, 300);
        }
        assert!(d.take_clip_request());
        // A door slam in a quiet room isn't music.
        let mut d = room(vec![]);
        let mut now = 0;
        d.update(now);
        feed(&mut d, -30.0, &mut now, 200);
        feed(&mut d, -50.0, &mut now, 8000);
        assert!(!d.take_clip_request());
    }

    #[test]
    fn room_listens_when_nothing_plays() {
        let mut off = Device::default();
        off.update(0);
        assert!(off.mode() == Mode::Off && !off.mic_wanted()); // not unless it's turned on
        let mut d = room(vec![]);
        let mut now = 0;
        d.update(now);
        assert!(d.mode() == Mode::Analog && d.room() && d.mic_wanted() && d.analog_source().is_none());
        feed(&mut d, -30.0, &mut now, 3000);
        assert!(!d.take_clip_request()); // a few loud seconds aren't music yet
        feed(&mut d, -30.0, &mut now, 3000);
        assert!(d.take_clip_request());
        d.recognition(&Recognition { matched: true, title: "Peg".into(), art_url: "https://art/aja.jpg".into(), ..Default::default() }, now);
        d.update(now);
        assert_eq!(d.art(), "https://art/aja.jpg");
    }

    #[test]
    fn room_steps_aside_for_any_playing_player() {
        // The TV is a digital source for its music apps only; Netflix playing still stops the room listening.
        let mut d = room(vec![digital("media_player.apple_tv", &["Music"])]);
        let mut tv = player("media_player.apple_tv", "playing", "", "");
        tv.app_name = "Netflix".into();
        d.player(tv.clone(), 0);
        d.update(0);
        assert!(d.mode() == Mode::Off && !d.mic_wanted());
        tv.state = "paused".into();
        d.player(tv.clone(), 1000);
        d.update(1000);
        assert!(d.room());
        let mut both = room(vec![digital("media_player.apple_tv", &[])]);
        tv.state = "playing".into();
        tv.app_name = "Music".into();
        tv.picture = "/api/x".into();
        both.player(tv, 0);
        both.update(0);
        assert_eq!((both.mode(), both.art()), (Mode::Digital, "/api/x"));
    }

    #[test]
    fn room_gives_up_until_it_goes_quiet() {
        let mut d = room(vec![]);
        let mut now = 0;
        feed(&mut d, -30.0, &mut now, 6000);
        assert!(d.take_clip_request());
        d.recognition(&miss("no_match"), now);
        feed(&mut d, -30.0, &mut now, 10500); // the one retry
        assert!(d.take_clip_request());
        d.recognition(&miss("no_match"), now);
        d.update(now);
        assert!(d.listener().given_up() && d.art().is_empty());
        // The TV carries on with pauses: none of them is a new song.
        feed(&mut d, -70.0, &mut now, 2000);
        feed(&mut d, -30.0, &mut now, 6000);
        assert!(!d.take_clip_request());
        // Quiet long enough to end a side, then music: listening again.
        feed(&mut d, -70.0, &mut now, 31000);
        feed(&mut d, -30.0, &mut now, 6000);
        assert!(d.take_clip_request() && !d.listener().given_up());
    }

    #[test]
    fn calibration() {
        let mut d = house();
        d.start_calibration(0, 3000);
        assert!(d.mic_wanted());
        let mut t = 0;
        while t <= 3000 {
            d.level(if t < 1500 { -60.0 } else { -30.0 }, t);
            t += 50;
        }
        let c = d.take_calibration().unwrap();
        assert!(!d.calibrating());
        assert_eq!((c.p10, c.p90, c.samples), (-60.0, -30.0, 61));
    }
}
