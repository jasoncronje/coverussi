//! The settings the portal edits and the display follows. Every field has a
//! default, so the display works before anyone opens the portal. The JSON shape
//! is the hub's, so the portal is unchanged.

use serde::{Deserialize, Serialize};

use crate::tz::posix_tz;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub display: Display,
    pub sources: Sources,
    pub listening: Listening,
    pub recognition: Recognition,
    pub home_assistant: HomeAssistant,
    pub idle: Idle,
    /// The web app's first-run setup was finished or skipped.
    pub setup_done: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Transition {
    #[default]
    Crossfade,
    FadeBlack,
    Slide,
    Wipe,
    Dissolve,
    Cut,
}

/// What a picture that isn't square, like a 16:9 video thumbnail, becomes on the square panel.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Fit {
    /// The middle square fills the panel.
    #[default]
    Crop,
    /// The whole picture, with black bars.
    Letterbox,
    /// Full height, sliding slowly to one side and back. Tall pictures are cropped.
    Pan,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Display {
    /// Off: always brightness_day, for brightness set from Home Assistant.
    pub brightness_schedule: bool,
    pub brightness_day: i64,
    pub brightness_night: i64,
    pub night_start: String,
    pub night_end: String,
    /// The POWER port gives at most 3 A.
    pub current_limit_ma: i64,
    /// How long a transition takes.
    pub crossfade_ms: i64,
    pub transition: Transition,
    pub fit: Fit,
    pub pause_grace_s: i64,
    pub side_end_dim_s: i64,
    /// IANA, for the night schedule.
    pub time_zone: String,
    /// The panel's colour tuning, set on the calibration screen.
    pub color: Color,
    /// Turns the picture clockwise for a panel mounted on its side or upside down: 0, 90, 180 or 270.
    pub rotation: i64,
    /// A recognised record's title and artist, between showings of its cover.
    pub song_screen: bool,
    pub song_cover_s: i64,
    pub song_text_s: i64,
    /// A record turning while one plays that isn't recognised yet; off, the idle screen.
    pub record: bool,
    /// 33, 45 or 78.
    pub record_rpm: i64,
    /// The label's colour, `#rrggbb`.
    pub record_label: String,
    /// The sheen brightens as the music gets louder.
    pub record_sheen: bool,
}

/// How the panel draws colour. The web app's mirror shows the art untouched.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Color {
    /// White balance: each colour's strength, in percent. The green LEDs are
    /// the brightest, so white can look green-yellow at 100.
    pub red_pct: i64,
    pub green_pct: i64,
    pub blue_pct: i64,
    /// Above 0 lifts dark and middle tones, below 0 deepens them; black and white stay.
    pub shadows: i64,
    /// 100 leaves colours as they are. LEDs are vivid, so covers can look loud.
    pub saturation_pct: i64,
}

impl Default for Color {
    fn default() -> Self {
        Self { red_pct: 100, green_pct: 100, blue_pct: 100, shadows: 0, saturation_pct: 100 }
    }
}

impl Color {
    pub fn validate(&self) -> Result<(), Invalid> {
        range("display.color.red_pct", self.red_pct, 50, 100)?;
        range("display.color.green_pct", self.green_pct, 50, 100)?;
        range("display.color.blue_pct", self.blue_pct, 50, 100)?;
        range("display.color.shadows", self.shadows, -50, 50)?;
        range("display.color.saturation_pct", self.saturation_pct, 50, 150)
    }
}

impl Display {
    /// One turn of the record, in ms.
    pub fn record_turn_ms(&self) -> u64 {
        if self.record_rpm == 33 { 1800 } else { 60_000 / self.record_rpm.max(1) as u64 }
    }

    /// The record's label colour, from `#rrggbb`.
    pub fn record_color(&self) -> Option<[u8; 3]> {
        let h = self.record_label.strip_prefix('#')?;
        if h.len() != 6 || !h.bytes().all(|b| b.is_ascii_hexdigit()) {
            return None;
        }
        let byte = |i: usize| u8::from_str_radix(&h[i..i + 2], 16).ok();
        Some([byte(0)?, byte(2)?, byte(4)?])
    }
}

impl Default for Display {
    fn default() -> Self {
        Self {
            brightness_schedule: true,
            brightness_day: 160,
            brightness_night: 40,
            night_start: "22:00".into(),
            night_end: "07:00".into(),
            current_limit_ma: 2400,
            crossfade_ms: 400,
            transition: Transition::Crossfade,
            fit: Fit::Crop,
            pause_grace_s: 30,
            side_end_dim_s: 120,
            time_zone: "UTC".into(),
            color: Color::default(),
            rotation: 0,
            song_screen: true,
            song_cover_s: 20,
            song_text_s: 6,
            record: true,
            record_rpm: 33,
            record_label: "#f2b33d".into(),
            record_sheen: true,
        }
    }
}

/// A smart player that tells Home Assistant what it plays, with artwork; the art shows while it plays.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct DigitalSource {
    pub entity: String,
    /// Apps or inputs (HA's app_name, app_id or source) that show art.
    #[serde(default)]
    pub apps: Vec<String>,
    /// Apps that show the film screen (Netflix on an Apple TV).
    #[serde(default)]
    pub film: Vec<String>,
    /// Apps that show nothing, the player ignored.
    #[serde(default)]
    pub off: Vec<String>,
    /// Older settings: `apps` names the apps that don't show art instead.
    #[serde(default)]
    pub exclude: bool,
    /// Apps on no list; None (older settings): art when `apps` is empty or excludes, else nothing.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub other: Option<AppShow>,
    /// Off: kept in the list but not followed.
    #[serde(default = "on")]
    pub enabled: bool,
}

/// What a digital source's app shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppShow {
    Art,
    Film,
    Off,
}

/// A device that can't say what it plays (a turntable, a tape deck, a CD player): the display listens.
///
/// It plays while the entity's attribute, or its state with attribute "state", has this value:
/// a streamer's source "Phono In", or a smart plug that's "on".
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AnalogSource {
    /// Shown in the portal and History, e.g. "Turntable".
    #[serde(default)]
    pub name: String,
    pub entity: String,
    #[serde(default = "source_attribute")]
    pub attribute: String,
    pub value: String,
    /// Off: kept in the list but not followed.
    #[serde(default = "on")]
    pub enabled: bool,
}

fn source_attribute() -> String {
    "source".into()
}

fn on() -> bool {
    true
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Sources {
    /// The first one playing wins; the list order is the priority.
    pub digital: Vec<DigitalSource>,
    /// The first one playing wins, over any digital source.
    pub analog: Vec<AnalogSource>,
    /// With nothing above playing, listen to the room and identify any music heard.
    pub listen_for_music: bool,
}

/// How the microphones hear an analog source. Levels in dBFS.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Listening {
    pub music_threshold_db: f64,
    pub gap_threshold_db: f64,
    pub min_gap_ms: i64,
    pub side_end_silence_s: i64,
    pub clip_ms: i64,
    pub retry_backoff_s: Vec<i64>,
    pub relisten_after_end_ms: i64,
    pub mic_gain_db: f64,
    /// Listening for music: loud this long before identifying.
    pub music_start_s: i64,
}

impl Default for Listening {
    fn default() -> Self {
        Self {
            music_threshold_db: -45.0,
            gap_threshold_db: -55.0,
            min_gap_ms: 800,
            side_end_silence_s: 30,
            clip_ms: 10_000,
            retry_backoff_s: vec![10, 20, 40],
            relisten_after_end_ms: 1000,
            mic_gain_db: 30.0,
            music_start_s: 5,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Recognition {
    pub min_interval_s: i64,
    /// Matches in a row on another album by the same artist.
    pub replace_after: i64,
    pub session_timeout_s: i64,
    /// When the match is a single, a compilation or a live record, show the
    /// song's studio album instead.
    pub prefer_album: bool,
    /// Debug: keep recent clips.
    pub keep_clips: bool,
    /// The Apple Music store covers and albums come from, as a two-letter
    /// code; empty: the time zone's country.
    pub store_country: String,
}

impl Default for Recognition {
    fn default() -> Self {
        Self {
            min_interval_s: 20,
            replace_after: 2,
            session_timeout_s: 900,
            prefer_album: true,
            keep_clips: false,
            store_country: String::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct HomeAssistant {
    pub mqtt_discovery: bool,
    pub name: String,
}

impl Default for HomeAssistant {
    fn default() -> Self {
        Self { mqtt_discovery: true, name: "Coverussi".into() }
    }
}

/// What the panel shows when there's no art.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum IdleShow {
    /// Dark.
    #[default]
    Off,
    Clock,
    Weather,
    /// A few Home Assistant entities' states.
    Entities,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Idle {
    pub show: IdleShow,
    /// A weather entity, for Weather.
    pub weather: String,
    /// For Entities, at most four.
    pub entities: Vec<IdleEntity>,
    pub clock_24h: bool,
}

impl Default for Idle {
    fn default() -> Self {
        Self { show: IdleShow::Off, weather: String::new(), entities: Vec::new(), clock_24h: true }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct IdleEntity {
    pub entity: String,
    /// Shown above the state; empty: Home Assistant's name.
    #[serde(default)]
    pub name: String,
}

/// A field the portal sent that doesn't fit, as `"display.brightness_day: ..."`.
#[derive(Debug, PartialEq)]
pub struct Invalid(pub String);

impl std::fmt::Display for Invalid {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl Settings {
    /// The Apple Music store to ask: the one set, else the time zone's country; empty: Apple's default.
    pub fn store_country(&self) -> String {
        let set = self.recognition.store_country.trim();
        let c = if set.is_empty() { crate::tz::country(&self.display.time_zone).unwrap_or("") } else { set };
        c.to_ascii_uppercase()
    }

    /// Parse and check a settings document.
    pub fn from_json(body: &str) -> Result<Self, Invalid> {
        let s: Self = serde_json::from_str(body).map_err(|e| Invalid(e.to_string()))?;
        s.validate()?;
        Ok(s)
    }

    pub fn validate(&self) -> Result<(), Invalid> {
        let d = &self.display;
        range("display.brightness_day", d.brightness_day, 0, 255)?;
        range("display.brightness_night", d.brightness_night, 0, 255)?;
        clock("display.night_start", &d.night_start)?;
        clock("display.night_end", &d.night_end)?;
        range("display.current_limit_ma", d.current_limit_ma, 500, 3000)?;
        range("display.crossfade_ms", d.crossfade_ms, 0, 3000)?;
        range("display.pause_grace_s", d.pause_grace_s, 0, 3600)?;
        range("display.side_end_dim_s", d.side_end_dim_s, 0, 3600)?;
        d.color.validate()?;
        if ![0, 90, 180, 270].contains(&d.rotation) {
            return Err(Invalid(format!("display.rotation: {} isn't 0, 90, 180 or 270", d.rotation)));
        }
        range("display.song_cover_s", d.song_cover_s, 5, 600)?;
        range("display.song_text_s", d.song_text_s, 2, 60)?;
        if ![33, 45, 78].contains(&d.record_rpm) {
            return Err(Invalid(format!("display.record_rpm: {} isn't 33, 45 or 78", d.record_rpm)));
        }
        if d.record_color().is_none() {
            return Err(Invalid(format!("display.record_label: {:?} isn't a colour like #f2b33d", d.record_label)));
        }
        if posix_tz(&d.time_zone).is_none() {
            return Err(Invalid(format!(
                "display.time_zone: unknown time zone {:?}; use an IANA name like Europe/London",
                d.time_zone
            )));
        }

        for (i, s) in self.sources.digital.iter().enumerate() {
            entity(&format!("sources.digital.{i}.entity"), &s.entity)?;
        }
        for (i, s) in self.sources.analog.iter().enumerate() {
            entity(&format!("sources.analog.{i}.entity"), &s.entity)?;
            not_empty(&format!("sources.analog.{i}.attribute"), &s.attribute)?;
            not_empty(&format!("sources.analog.{i}.value"), &s.value)?;
        }

        let l = &self.listening;
        frange("listening.music_threshold_db", l.music_threshold_db, -90.0, 0.0)?;
        frange("listening.gap_threshold_db", l.gap_threshold_db, -90.0, 0.0)?;
        range("listening.min_gap_ms", l.min_gap_ms, 100, 10_000)?;
        range("listening.side_end_silence_s", l.side_end_silence_s, 5, 600)?;
        range("listening.clip_ms", l.clip_ms, 3000, 20_000)?;
        range("listening.relisten_after_end_ms", l.relisten_after_end_ms, 0, 60_000)?;
        frange("listening.mic_gain_db", l.mic_gain_db, 0.0, 37.5)?;
        range("listening.music_start_s", l.music_start_s, 1, 60)?;

        let r = &self.recognition;
        range("recognition.min_interval_s", r.min_interval_s, 5, 600)?;
        range("recognition.replace_after", r.replace_after, 1, 5)?;
        range("recognition.session_timeout_s", r.session_timeout_s, 60, 7200)?;
        let c = &r.store_country;
        if !c.is_empty() && !(c.len() == 2 && c.bytes().all(|b| b.is_ascii_alphabetic())) {
            return Err(Invalid(format!("recognition.store_country: {c:?} isn't a two-letter country code like US")));
        }

        let i = &self.idle;
        if i.show == IdleShow::Weather || !i.weather.is_empty() {
            entity("idle.weather", &i.weather)?;
        }
        if i.entities.len() > crate::idle::MAX_ENTITIES {
            return Err(Invalid(format!("idle.entities: {} is more than {}", i.entities.len(), crate::idle::MAX_ENTITIES)));
        }
        for (n, e) in i.entities.iter().enumerate() {
            entity(&format!("idle.entities.{n}.entity"), &e.entity)?;
        }
        Ok(())
    }
}

fn range(field: &str, v: i64, lo: i64, hi: i64) -> Result<(), Invalid> {
    if (lo..=hi).contains(&v) { Ok(()) } else { Err(Invalid(format!("{field}: {v} is outside {lo} to {hi}"))) }
}

fn frange(field: &str, v: f64, lo: f64, hi: f64) -> Result<(), Invalid> {
    if (lo..=hi).contains(&v) { Ok(()) } else { Err(Invalid(format!("{field}: {v} is outside {lo} to {hi}"))) }
}

fn not_empty(field: &str, v: &str) -> Result<(), Invalid> {
    if v.is_empty() { Err(Invalid(format!("{field}: empty"))) } else { Ok(()) }
}

/// `HH:MM`, two digits each.
fn clock(field: &str, v: &str) -> Result<(), Invalid> {
    let b = v.as_bytes();
    let ok = b.len() == 5 && b[2] == b':' && [0, 1, 3, 4].iter().all(|&i| b[i].is_ascii_digit());
    if ok { Ok(()) } else { Err(Invalid(format!("{field}: {v:?} is not HH:MM"))) }
}

/// A Home Assistant entity ID: `domain.object`, lower case, digits and `_`.
fn entity(field: &str, v: &str) -> Result<(), Invalid> {
    let part = |p: &str| !p.is_empty() && p.bytes().all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_');
    match v.split_once('.') {
        Some((domain, object)) if part(domain) && part(object) => Ok(()),
        _ => Err(Invalid(format!("{field}: {v:?} is not an entity ID like media_player.living_room"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn parse(v: serde_json::Value) -> Result<Settings, Invalid> {
        Settings::from_json(&v.to_string())
    }

    #[test]
    fn defaults_have_no_sources_and_round_trip() {
        let s = Settings::default();
        assert!(s.sources.digital.is_empty() && s.sources.analog.is_empty());
        assert_eq!(parse(serde_json::to_value(&s).unwrap()).unwrap(), s);
        assert_eq!(parse(json!({})).unwrap(), s);
    }

    #[test]
    fn hub_json_reads_the_same() {
        // A document as the hub saved it, unknown fields and all.
        let s = parse(json!({
            "display": {"brightness_day": 77, "transition": "fade_black", "time_zone": "Europe/London"},
            "sources": {
                "digital": [{"entity": "media_player.wiim_ultra_2", "apps": []},
                            {"entity": "media_player.apple_tv", "apps": ["Music", "Spotify"]}],
                "analog": [{"name": "Turntable", "entity": "media_player.wiim_ultra_2", "attribute": "source", "value": "Phono In"}],
                "listen_for_music": false
            },
            "listening": {"music_threshold_db": -41},
            "something_new": 1
        }))
        .unwrap();
        assert_eq!(s.display.brightness_day, 77);
        assert_eq!(s.display.transition, Transition::FadeBlack);
        assert_eq!(s.display.brightness_night, 40);
        assert_eq!(s.sources.digital[1].apps, ["Music", "Spotify"]);
        assert!(s.sources.digital[1].film.is_empty() && s.sources.digital[1].other.is_none());
        assert_eq!(s.listening.music_threshold_db, -41.0);
    }

    #[test]
    fn apps_can_show_the_film_screen() {
        let s = parse(json!({"sources": {"digital": [
            {"entity": "media_player.apple_tv", "apps": ["Music"], "film": ["Netflix"], "off": ["YouTube"], "other": "film"}
        ]}}))
        .unwrap();
        let d = &s.sources.digital[0];
        assert_eq!((d.film.as_slice(), d.off.as_slice(), d.other), (&["Netflix".to_string()][..], &["YouTube".to_string()][..], Some(AppShow::Film)));
        assert_eq!(parse(serde_json::to_value(&s).unwrap()).unwrap(), s);
    }

    #[test]
    fn bad_sources_are_refused() {
        for sources in [
            json!({"digital": [{"entity": ""}]}),
            json!({"digital": [{"entity": "Media Player.TV"}]}),
            json!({"digital": [{"apps": ["Music"]}]}),
            json!({"analog": [{"entity": "media_player.wiim", "value": ""}]}),
            json!({"analog": [{"entity": "media_player.wiim", "attribute": "", "value": "Phono"}]}),
            json!({"analog": [{"entity": "not an entity", "value": "Phono"}]}),
            json!({"analog": [{"name": "Tape deck", "value": "on"}]}),
        ] {
            assert!(parse(json!({"sources": sources})).is_err(), "{sources}");
        }
    }

    #[test]
    fn other_setups_are_accepted() {
        let s = parse(json!({"sources": {
            "digital": [{"entity": "media_player.sonos_living_room", "apps": ["Spotify"]}],
            "analog": [
                {"name": "Turntable", "entity": "switch.turntable", "attribute": "state", "value": "on"},
                {"name": "Tape deck", "entity": "media_player.receiver", "value": "TAPE"},
                {"entity": "media_player.receiver", "attribute": "sound_mode", "value": "CD"}
            ]
        }}))
        .unwrap();
        let attrs: Vec<_> = s.sources.analog.iter().map(|a| a.attribute.as_str()).collect();
        assert_eq!(attrs, ["state", "source", "sound_mode"]);
        assert_eq!(s.sources.analog[2].name, "");
        assert!(s.sources.digital[0].enabled && s.sources.analog[0].enabled); // on unless turned off
    }

    #[test]
    fn ranges_times_and_zones_are_checked() {
        assert!(parse(json!({"display": {"brightness_day": 256}})).is_err());
        assert!(parse(json!({"display": {"current_limit_ma": 3500}})).is_err());
        assert!(parse(json!({"display": {"night_start": "7:00"}})).is_err());
        assert!(parse(json!({"display": {"transition": "spin"}})).is_err());
        assert!(parse(json!({"listening": {"mic_gain_db": 40}})).is_err());
        assert!(parse(json!({"recognition": {"replace_after": 0}})).is_err());
        let e = parse(json!({"display": {"time_zone": "Mars/Olympus"}})).unwrap_err();
        assert!(e.0.starts_with("display.time_zone: unknown time zone"), "{e}");
    }

    #[test]
    fn song_screen_and_record() {
        let d = Settings::default().display;
        assert_eq!((d.song_screen, d.song_cover_s, d.song_text_s, d.record_turn_ms(), d.record_color()), (true, 20, 6, 1800, Some([242, 179, 61])));
        let s = parse(json!({"display": {"record_rpm": 45, "record_label": "#FF0080"}})).unwrap();
        assert_eq!((s.display.record_turn_ms(), s.display.record_color()), (1333, Some([255, 0, 128])));
        assert!(parse(json!({"display": {"record_rpm": 16}})).is_err());
        assert!(parse(json!({"display": {"record_label": "amber"}})).is_err());
        assert!(parse(json!({"display": {"song_text_s": 1}})).is_err());
    }

    #[test]
    fn idle_screen() {
        let s = parse(json!({"idle": {"show": "entities", "entities": [{"entity": "sensor.outside"}]}})).unwrap();
        assert_eq!((s.idle.show, s.idle.entities[0].name.as_str(), s.idle.clock_24h), (IdleShow::Entities, "", true));
        assert!(parse(json!({"idle": {"show": "weather"}})).is_err());
        assert!(parse(json!({"idle": {"show": "weather", "weather": "weather.home"}})).is_ok());
        assert!(parse(json!({"idle": {"show": "slideshow"}})).is_err());
        let five = vec![json!({"entity": "sensor.a"}); 5];
        assert!(parse(json!({"idle": {"entities": five}})).is_err());
        assert!(parse(json!({"idle": {"entities": [{"entity": "Outside"}]}})).is_err());
    }
}
