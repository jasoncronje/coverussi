//! What the panel shows when there's no art: a clock, the weather or a few
//! Home Assistant entities, drawn in two pixel fonts. Also the start-up screen
//! and the progress screen.

use serde_json::Value;

use crate::art::{Rgb, SIZE};
use crate::ha::HaStates;
use crate::model::{Ms, Player};
use crate::settings::{Idle, IdleShow};

/// The local time, once the clock is set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct LocalTime {
    /// Minutes after midnight.
    pub minute: u32,
    /// 0 is Sunday.
    pub weekday: u32,
    pub day: u32,
    /// 1 is January.
    pub month: u32,
    /// Unix seconds.
    pub unix: u64,
}

/// The most entities the screen fits.
pub const MAX_ENTITIES: usize = 4;

const WHITE: [u8; 3] = [255, 255, 255];
const GRAY: [u8; 3] = [150, 150, 150];
/// The logo's pink (docs/logo).
const PINK: [u8; 3] = [233, 79, 149];
/// An unlit LED in the wordmark's style.
const UNLIT: [u8; 3] = [38, 38, 38];
/// An update that failed.
const RED: [u8; 3] = [229, 86, 111];
/// The progress screen's ring to come.
const DIM: [u8; 3] = [42, 42, 42];
/// Pixels in the 1 px border the ring runs round.
const RING: u32 = 4 * (SIZE as u32 - 1);

const DAYS: [&str; 7] = ["SUN", "MON", "TUE", "WED", "THU", "FRI", "SAT"];
const MONTHS: [&str; 12] = ["JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC"];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Icon {
    Sun,
    Moon,
    PartlyCloudy,
    Cloud,
    Rain,
    Pouring,
    Snow,
    Sleet,
    Hail,
    Storm,
    Fog,
    Wind,
    Unknown,
}

/// What a screen says; the engine redraws only when it changes.
#[derive(Clone, Debug, PartialEq)]
pub enum Content {
    /// `ampm` is "AM" or "PM" on a 12-hour clock, else empty; it's drawn beside the time.
    Clock { time: String, ampm: String, date: String },
    Weather { icon: Icon, temperature: String, condition: String, time: String, ampm: String },
    /// (label, value) rows.
    Entities(Vec<(String, String)>),
    /// An app on the progress screen: the idle screen (the clock when that's off) inside a ring with this many
    /// of its pixels lit in the app's colour, for how far into the video or song it is.
    Progress { screen: Box<Content>, lit: u32, color: [u8; 3] },
}

/// The screen to show, or None to leave the panel dark.
pub fn content(idle: &Idle, ha: &HaStates, clock: Option<LocalTime>) -> Option<Content> {
    let time = || clock.map_or("--:--".to_string(), |c| clock_text(c.minute, idle.clock_24h));
    let ampm = || ampm_text(clock, idle.clock_24h);
    match idle.show {
        IdleShow::Off => None,
        IdleShow::Clock => Some(Content::Clock {
            time: time(),
            ampm: ampm(),
            date: clock
                .map(|c| format!("{} {} {}", DAYS[c.weekday as usize % 7], c.day, MONTHS[(c.month as usize + 11) % 12]))
                .unwrap_or_default(),
        }),
        IdleShow::Weather => {
            let (state, temperature) = match ha.get(&idle.weather) {
                Some((state, attributes)) => (state, attributes.get("temperature").and_then(Value::as_f64)),
                None => ("", None),
            };
            let (icon, condition) = condition(state);
            Some(Content::Weather {
                icon,
                temperature: temperature.map(|t| format!("{}°", t.round() as i64)).unwrap_or_default(),
                condition: condition.to_string(),
                time: time(),
                ampm: ampm(),
            })
        }
        IdleShow::Entities => Some(Content::Entities(
            idle.entities
                .iter()
                .take(MAX_ENTITIES)
                .map(|e| {
                    let found = ha.get(&e.entity);
                    let name = found.and_then(|(_, a)| a.get("friendly_name")).and_then(Value::as_str);
                    let label = if !e.name.is_empty() { e.name.as_str() } else { name.unwrap_or(&e.entity) };
                    let value = found.map_or("--".to_string(), |(state, a)| {
                        entity_value(state, a.get("unit_of_measurement").and_then(Value::as_str).unwrap_or(""))
                    });
                    (label.to_uppercase(), value)
                })
                .collect(),
        )),
    }
}

/// The progress screen for the player shown: the idle screen, with the ring for its progress in `color`.
pub fn progress(idle: &Idle, ha: &HaStates, p: Option<&Player>, clock: Option<LocalTime>, color: [u8; 3]) -> Content {
    let shown = if idle.show == IdleShow::Off { Idle { show: IdleShow::Clock, ..idle.clone() } } else { idle.clone() };
    let screen = content(&shown, ha, clock).unwrap_or(Content::Entities(Vec::new()));
    let at = p.and_then(|p| p.position_now(clock.map_or(0.0, |c| c.unix as f64)));
    let duration = p.and_then(|p| p.duration).filter(|d| *d > 0.0);
    let lit = match (at, duration) {
        (Some(at), Some(d)) => ((at / d).clamp(0.0, 1.0) * RING as f64) as u32,
        _ => 0,
    };
    Content::Progress { screen: Box::new(screen), lit, color }
}

/// When the ring goes back (the next video, the start again), it runs down to nothing, then fills
/// up to where the new one is.
const DRAIN_MS: Ms = 600;
const FILL_MS: Ms = 800;

/// The ring as the panel shows it, running down and filling up again rather than jumping back.
#[derive(Default)]
pub struct Ring {
    shown: u32,
    /// When it started running down, and from how many pixels.
    drain: Option<(Ms, u32)>,
}

impl Ring {
    /// The lit pixels to show for the `lit` now; `on` is whether the progress screen is already up.
    pub fn lit(&mut self, lit: u32, now: Ms, on: bool) -> u32 {
        if !on {
            self.drain = None;
        }
        // Filling up follows the playback, so only a ring at rest runs down.
        if on && self.drain.is_none() && lit + 2 < self.shown {
            self.drain = Some((now, self.shown));
        }
        let ease = |t: f32| t * t * (3.0 - 2.0 * t);
        self.shown = match self.drain {
            Some((at, from)) if now - at < DRAIN_MS => {
                (from as f32 * (1.0 - ease((now - at) as f32 / DRAIN_MS as f32))) as u32
            }
            Some((at, _)) if now - at < DRAIN_MS + FILL_MS => {
                (lit as f32 * ease((now - at - DRAIN_MS) as f32 / FILL_MS as f32)) as u32
            }
            _ => {
                self.drain = None;
                lit
            }
        };
        self.shown
    }
}

/// The border pixel `i` steps clockwise from the top middle.
fn ring(i: u32) -> (i32, i32) {
    let (i, n) = ((i + SIZE as u32 / 2) % RING, SIZE as i32 - 1);
    let i = i as i32;
    match i / n {
        0 => (i, 0),
        1 => (n, i - n),
        2 => (3 * n - i, n),
        _ => (0, 4 * n - i),
    }
}

/// "AM" or "PM" on a 12-hour clock that's set, else empty.
fn ampm_text(clock: Option<LocalTime>, h24: bool) -> String {
    match clock {
        Some(c) if !h24 => (if c.minute / 60 % 24 < 12 { "AM" } else { "PM" }).into(),
        _ => String::new(),
    }
}

fn clock_text(minute: u32, h24: bool) -> String {
    let (h, m) = (minute / 60 % 24, minute % 60);
    if h24 { format!("{h:02}:{m:02}") } else { format!("{}:{m:02}", (h + 11) % 12 + 1) }
}

/// Home Assistant's weather conditions.
fn condition(state: &str) -> (Icon, &'static str) {
    match state {
        "sunny" => (Icon::Sun, "SUNNY"),
        "clear-night" => (Icon::Moon, "CLEAR"),
        "partlycloudy" => (Icon::PartlyCloudy, "PARTLY CLOUDY"),
        "cloudy" => (Icon::Cloud, "CLOUDY"),
        "rainy" => (Icon::Rain, "RAIN"),
        "pouring" => (Icon::Pouring, "POURING"),
        "snowy" => (Icon::Snow, "SNOW"),
        "snowy-rainy" => (Icon::Sleet, "SLEET"),
        "hail" => (Icon::Hail, "HAIL"),
        "lightning" | "lightning-rainy" => (Icon::Storm, "STORM"),
        "fog" => (Icon::Fog, "FOG"),
        "windy" | "windy-variant" => (Icon::Wind, "WINDY"),
        "exceptional" => (Icon::Unknown, "WARNING"),
        "" => (Icon::Unknown, ""),
        _ => (Icon::Unknown, "--"),
    }
}

/// A state with its unit: "21.4°C", "45%", "1.2 KWH", "ON". Long decimals keep one place.
fn entity_value(state: &str, unit: &str) -> String {
    if state == "unavailable" || state == "unknown" {
        return "--".into();
    }
    let number = match state.parse::<f64>() {
        Ok(v) if state.split_once('.').is_some_and(|(_, d)| d.len() > 1) => format!("{v:.1}"),
        _ => state.to_string(),
    };
    let space = if unit.is_empty() || unit.starts_with('°') || unit == "%" { "" } else { " " };
    format!("{number}{space}{unit}").to_uppercase()
}

/// Draw a screen.
pub fn draw(c: &Content) -> Box<Rgb> {
    if let Content::Progress { screen, lit, color } = c {
        let mut p = Canvas(draw(screen));
        for i in 0..RING {
            let (x, y) = ring(i);
            p.set(x, y, if i < *lit { *color } else { DIM });
        }
        return p.0;
    }
    let mut p = Canvas(Box::new([0; SIZE * SIZE * 3]));
    match c {
        Content::Clock { time, ampm, date } => {
            p.time(time, ampm, Font::Big, 2, 19);
            p.centered(date, Font::Small, 1, 39, GRAY);
        }
        Content::Weather { icon, temperature, condition, time, ampm } => {
            p.icon(*icon, 15, 13);
            let scale = if Font::Big.width(temperature, 2) <= 32 { 2 } else { 1 };
            let w = Font::Big.width(temperature, scale);
            p.text(temperature, Font::Big, scale, 62 - w, if scale == 2 { 7 } else { 10 }, WHITE);
            p.centered(condition, Font::Small, 1, 31, GRAY);
            p.time(time, ampm, Font::Big, 2, 43);
        }
        Content::Entities(rows) => {
            let top = (SIZE as i32 - rows.len() as i32 * 16 + 2) / 2;
            for (i, (label, value)) in rows.iter().enumerate() {
                let y = top + i as i32 * 16;
                p.text(&fit(label, Font::Small, 1), Font::Small, 1, 2, y, GRAY);
                p.text(&fit(value, Font::Big, 1), Font::Big, 1, 2, y + 7, WHITE);
            }
        }
        Content::Progress { .. } => unreachable!("drawn above"),
    }
    p.0
}

/// What the board waits for while it starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Boot {
    WiFi,
    HomeAssistant,
    Ready,
}

const BAR: (i32, i32, i32) = (14, 50, 31); // left, right, top
const SWEEP_MS: u64 = 1400;

/// The start-up screen `t` ms in: the wordmark, a bar sweeping until it's ready, and what it waits for.
pub fn boot(stage: Boot, t: u64) -> Box<Rgb> {
    let mut p = wordmark();
    p.bar(if stage == Boot::Ready { BAR.0..BAR.1 } else { sweep(t) }, PINK);
    let label = match stage {
        Boot::WiFi => "WI-FI",
        Boot::HomeAssistant => "HOME ASSISTANT",
        Boot::Ready => "READY",
    };
    p.centered(label, Font::Small, 1, 39, GRAY);
    p.0
}

/// Where an update from GitHub is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Update {
    /// Asking GitHub for the release and connecting to the download.
    Starting,
    /// How much of the image is written, in percent.
    Downloading(u8),
    Restarting,
    Failed,
}

/// The update screen `t` ms in: the wordmark, a bar that fills with the
/// download (sweeping until it starts), and what's happening.
pub fn update(stage: Update, t: u64) -> Box<Rgb> {
    let mut p = wordmark();
    let (left, right, _) = BAR;
    let (lit, color) = match stage {
        Update::Starting => (sweep(t), PINK),
        Update::Downloading(pct) => (left..left + (right - left) * pct.min(100) as i32 / 100, PINK),
        Update::Restarting => (left..right, PINK),
        Update::Failed => (left..right, RED),
    };
    p.bar(lit, color);
    let label = match stage {
        Update::Starting => "UPDATING".to_string(),
        Update::Downloading(pct) => format!("UPDATING {}%", pct.min(100)),
        Update::Restarting => "RESTARTING".into(),
        Update::Failed => "UPDATE FAILED".into(),
    };
    p.centered(&label, Font::Small, 1, 39, if stage == Update::Failed { RED } else { GRAY });
    p.0
}

/// "COVER" pink and "USSI" white, across the top half.
fn wordmark() -> Canvas {
    let mut p = Canvas(Box::new([0; SIZE * SIZE * 3]));
    let x = (SIZE as i32 - Font::Big.width("COVERUSSI", 1)) / 2;
    p.text("COVER", Font::Big, 1, x, 18, PINK);
    p.text("USSI", Font::Big, 1, x + Font::Big.width("COVER", 1) + 1, 18, WHITE);
    p
}

/// A 10-pixel segment of the bar, back and forth.
fn sweep(t: u64) -> std::ops::Range<i32> {
    let (left, right, _) = BAR;
    let span = (right - left - 10) as u64;
    let phase = t % SWEEP_MS * 2 * span / SWEEP_MS;
    let at = left + if phase <= span { phase } else { 2 * span - phase } as i32;
    at..at + 10
}

const RECORD_R: f32 = 30.5;
const LABEL_R: f32 = 10.5;

/// While a record plays that isn't recognised (yet): the record turning once
/// every `turn_ms`, `t` ms in, its label `label`. Its sheen brightens with
/// `loudness`, 0 to 1.
pub fn record(t: u64, turn_ms: u64, label: [u8; 3], loudness: f32) -> Box<Rgb> {
    use std::f32::consts::{PI, TAU};
    let mut p = Canvas(Box::new([0; SIZE * SIZE * 3]));
    let turn = (t % turn_ms) as f32 / turn_ms as f32 * TAU;
    let (sin, cos) = turn.sin_cos();
    let mid = (SIZE as f32 - 1.0) / 2.0;
    let sheen_max = 30.0 + 60.0 * loudness.clamp(0.0, 1.0);
    for y in 0..SIZE as i32 {
        for x in 0..SIZE as i32 {
            let (dx, dy) = (x as f32 - mid, y as f32 - mid);
            let r = dx.hypot(dy);
            let edge = (RECORD_R + 0.5 - r).clamp(0.0, 1.0);
            if edge == 0.0 || r < 1.2 {
                continue; // off the record, or the spindle hole
            }
            // The vinyl: fine grooves, a lighter rim, a sheen from the top left that stays put.
            let groove = if (r * 1.5) as i32 % 2 == 0 { 1.0 } else { 0.75 };
            let rim = if r > RECORD_R - 1.0 { 14.0 } else { 0.0 };
            let sheen = (2.0 * (dy.atan2(dx) + 0.75 * PI)).cos().max(0.0).powi(6) * sheen_max;
            let v = 16.0 + rim + sheen * groove;
            let vinyl = [v, v, v * 1.06];
            // The label turns: two printed lines on its colour.
            let (u, w) = (dx * cos + dy * sin, -dx * sin + dy * cos);
            let printed = (u.abs() <= 6.0 && (2.5..=4.0).contains(&w)) || (u.abs() <= 4.0 && (-4.5..=-3.0).contains(&w));
            let label = if printed { [255.0, 236.0, 196.0] } else { label.map(f32::from) };
            let on_label = (LABEL_R + 0.5 - r).clamp(0.0, 1.0);
            let mut c = [0u8; 3];
            for k in 0..3 {
                c[k] = ((vinyl[k] + (label[k] - vinyl[k]) * on_label) * edge).round() as u8;
            }
            p.set(x, y, c);
        }
    }
    p.0
}

/// A recognised song between showings of its cover: the title, up to three
/// lines, over the artist, up to two. A featured artist is left off the title.
pub fn song(title: &str, artist: &str) -> Box<Rgb> {
    let mut p = Canvas(Box::new([0; SIZE * SIZE * 3]));
    let lower = title.to_lowercase();
    let featuring = ["(feat", "[feat", "(ft.", "[ft.", "(with "].iter().filter_map(|m| lower.find(m)).min();
    let title = wrap(featuring.map_or(title, |i| title[..i].trim_end()), Font::Big, 3);
    let artist = wrap(artist, Font::Small, 2);
    let gap = if title.is_empty() || artist.is_empty() { 0 } else { 4 };
    let height = title.len() as i32 * 9 - 2 + gap + artist.len() as i32 * 7 - 2;
    let mut y = (SIZE as i32 - height) / 2;
    for line in &title {
        p.centered(line, Font::Big, 1, y, WHITE);
        y += 9;
    }
    y += gap - 2;
    for line in &artist {
        p.centered(line, Font::Small, 1, y, GRAY);
        y += 7;
    }
    p.0
}

/// Words to lines that fit the panel, at most `max`; the last cut short if need be.
fn wrap(s: &str, font: Font, max: usize) -> Vec<String> {
    let s = s.replace(['’', '‘'], "'").replace(['“', '”'], "\"");
    let fits = |l: &str| font.width(l, 1) <= SIZE as i32 - 4;
    let mut lines: Vec<String> = Vec::new();
    for word in s.split_whitespace() {
        match lines.last_mut() {
            Some(l) if fits(&format!("{l} {word}")) => *l = format!("{l} {word}"),
            _ => lines.push(fit(word, font, 1)),
        }
    }
    if lines.len() > max {
        lines.truncate(max);
        let last = &mut lines[max - 1];
        while !last.is_empty() && !fits(&format!("{last}...")) {
            last.pop();
        }
        *last = format!("{}...", last.trim_end());
    }
    lines
}

/// Cut text to the panel's width, less a margin each side.
fn fit(s: &str, font: Font, scale: i32) -> String {
    let mut s: Vec<char> = s.chars().collect();
    while !s.is_empty() && font.width(&s.iter().collect::<String>(), scale) > SIZE as i32 - 4 {
        s.pop();
    }
    s.into_iter().collect()
}

#[derive(Clone, Copy)]
enum Font {
    /// 3 × 5, for labels.
    Small,
    /// 5 × 7, for values and, doubled, the time.
    Big,
}

struct Glyph {
    rows: &'static [u8],
    /// Bits are columns, the highest the left one.
    cell: u32,
}

impl Glyph {
    /// The columns drawn, left and width. Digits keep the whole cell, so the time doesn't shift.
    fn columns(&self, ch: char) -> (u32, u32) {
        if ch.is_ascii_digit() || ch == ' ' {
            return (0, if ch == ' ' { 2 } else { self.cell });
        }
        let used = self.rows.iter().fold(0u8, |a, r| a | r);
        if used == 0 {
            return (0, 2);
        }
        let left = (0..self.cell).find(|c| used >> (self.cell - 1 - c) & 1 == 1).unwrap();
        let right = (0..self.cell).rev().find(|c| used >> (self.cell - 1 - c) & 1 == 1).unwrap();
        (left, right - left + 1)
    }
}

impl Font {
    /// The big font falls back to the small one for what it doesn't have.
    fn glyph(self, ch: char) -> (Glyph, u32) {
        let ch = ch.to_ascii_uppercase();
        if let Font::Big = self {
            if let Some((_, rows)) = BIG.iter().find(|(c, _)| *c == ch) {
                return (Glyph { rows, cell: 5 }, 7);
            }
        }
        let rows = SMALL.iter().find(|(c, _)| *c == ch).or_else(|| SMALL.iter().find(|(c, _)| *c == '?')).unwrap();
        (Glyph { rows: &rows.1, cell: 3 }, 5)
    }

    fn height(self) -> u32 {
        match self {
            Font::Small => 5,
            Font::Big => 7,
        }
    }

    fn width(self, s: &str, scale: i32) -> i32 {
        let w: i32 = s.chars().map(|ch| self.glyph(ch).0.columns(ch.to_ascii_uppercase()).1 as i32 + 1).sum();
        (w - 1).max(0) * scale
    }
}

struct Canvas(Box<Rgb>);

impl Canvas {
    fn set(&mut self, x: i32, y: i32, c: [u8; 3]) {
        if (0..SIZE as i32).contains(&x) && (0..SIZE as i32).contains(&y) {
            self.0[(y as usize * SIZE + x as usize) * 3..][..3].copy_from_slice(&c);
        }
    }

    fn text(&mut self, s: &str, font: Font, scale: i32, mut x: i32, y: i32, c: [u8; 3]) {
        for ch in s.chars() {
            let (g, height) = font.glyph(ch);
            let (left, width) = g.columns(ch.to_ascii_uppercase());
            // A small glyph in the big font sits on the baseline; quote marks hang from the top.
            let drop = if matches!(ch, '\'' | '"') { 0 } else { font.height() - height };
            let top = y + drop as i32 * scale;
            for (row, bits) in g.rows.iter().enumerate() {
                for col in 0..width {
                    if bits >> (g.cell - 1 - left - col) & 1 == 1 {
                        for dy in 0..scale {
                            for dx in 0..scale {
                                self.set(x + col as i32 * scale + dx, top + row as i32 * scale + dy, c);
                            }
                        }
                    }
                }
            }
            x += (width as i32 + 1) * scale;
        }
    }

    /// The progress bar under the wordmark, `lit` in `c` and the rest unlit.
    fn bar(&mut self, lit: std::ops::Range<i32>, c: [u8; 3]) {
        let (left, right, top) = BAR;
        for x in left..right {
            for y in top..top + 2 {
                self.set(x, y, if lit.contains(&x) { c } else { UNLIT });
            }
        }
    }

    /// The time in white with AM/PM small and grey beside it, on its baseline, the two
    /// centred together. Too wide for that ("12:58"), the letters stack: A over M.
    fn time(&mut self, time: &str, ampm: &str, font: Font, scale: i32, y: i32) {
        let w = font.width(time, scale);
        let bottom = y + font.height() as i32 * scale;
        let beside = Font::Small.width(ampm, 1);
        let stack = w + 2 + beside > SIZE as i32 - 8;
        let gap = if ampm.is_empty() { 0 } else if stack { 2 + Font::Small.width("M", 1) } else { 2 + beside };
        let x = (SIZE as i32 - w - gap) / 2;
        self.text(time, font, scale, x, y, WHITE);
        if stack {
            for (i, ch) in ampm.chars().enumerate() {
                self.text(&ch.to_string(), Font::Small, 1, x + w + 2, bottom - 11 + i as i32 * 6, GRAY);
            }
        } else {
            self.text(ampm, Font::Small, 1, x + w + 2, bottom - 5, GRAY);
        }
    }

    fn centered(&mut self, s: &str, font: Font, scale: i32, y: i32, c: [u8; 3]) {
        let x = (SIZE as i32 - font.width(s, scale)) / 2;
        self.text(s, font, scale, x, y, c);
    }

    fn disc(&mut self, cx: i32, cy: i32, r: f32, c: [u8; 3]) {
        let n = r.ceil() as i32;
        for dy in -n..=n {
            for dx in -n..=n {
                if ((dx * dx + dy * dy) as f32) <= (r + 0.3) * (r + 0.3) {
                    self.set(cx + dx, cy + dy, c);
                }
            }
        }
    }

    fn line(&mut self, (x0, y0): (i32, i32), (x1, y1): (i32, i32), c: [u8; 3]) {
        let n = (x1 - x0).abs().max((y1 - y0).abs()).max(1);
        for i in 0..=n {
            self.set(x0 + (x1 - x0) * i / n, y0 + (y1 - y0) * i / n, c);
        }
    }

    fn sun(&mut self, cx: i32, cy: i32, r: i32) {
        const SUN: [u8; 3] = [255, 190, 30];
        self.disc(cx, cy, r as f32, SUN);
        let (near, far) = (r + 2, r + 3);
        for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
            self.line((cx + dx * near, cy + dy * near), (cx + dx * far, cy + dy * far), SUN);
        }
        let (near, far) = ((near * 7 + 5) / 10, (far * 7 + 5) / 10);
        for (dx, dy) in [(1, 1), (-1, 1), (1, -1), (-1, -1)] {
            self.line((cx + dx * near, cy + dy * near), (cx + dx * far, cy + dy * far), SUN);
        }
    }

    /// About 19 wide and 12 high, flat at cy + 5.
    fn cloud(&mut self, cx: i32, cy: i32, c: [u8; 3]) {
        self.disc(cx - 5, cy + 1, 4.0, c);
        self.disc(cx + 1, cy - 2, 5.0, c);
        self.disc(cx + 6, cy + 1, 4.0, c);
        for y in cy + 1..=cy + 5 {
            self.line((cx - 5, y), (cx + 6, y), c);
        }
    }

    fn icon(&mut self, icon: Icon, cx: i32, cy: i32) {
        const CLOUD: [u8; 3] = [190, 195, 205];
        const DARK: [u8; 3] = [120, 125, 140];
        const RAIN: [u8; 3] = [70, 140, 255];
        const BOLT: [u8; 3] = [255, 210, 40];
        let drops = |p: &mut Canvas, n: i32, c: [u8; 3]| {
            for i in 0..n {
                let x = cx - 5 + i * 10 / (n - 1).max(1);
                p.line((x + 1, cy + 8), (x - 1, cy + 11), c);
            }
        };
        let flakes = |p: &mut Canvas, c: [u8; 3]| {
            for (i, x) in [cx - 5, cx, cx + 5].into_iter().enumerate() {
                let y = cy + 9 + (i as i32 % 2) * 2;
                p.set(x, y, c);
                for (dx, dy) in [(1, 0), (-1, 0), (0, 1), (0, -1)] {
                    p.set(x + dx, y + dy, c);
                }
            }
        };
        match icon {
            Icon::Sun => self.sun(cx, cy + 2, 6),
            Icon::Moon => {
                self.disc(cx, cy + 2, 7.0, [225, 225, 200]);
                self.disc(cx + 4, cy - 1, 6.0, [0, 0, 0]);
            }
            Icon::PartlyCloudy => {
                self.sun(cx + 4, cy - 3, 4);
                self.cloud(cx - 2, cy + 4, CLOUD);
            }
            Icon::Cloud => self.cloud(cx, cy + 2, CLOUD),
            Icon::Rain | Icon::Pouring => {
                self.cloud(cx, cy, DARK);
                drops(self, if icon == Icon::Rain { 3 } else { 5 }, RAIN);
            }
            Icon::Snow => {
                self.cloud(cx, cy, CLOUD);
                flakes(self, WHITE);
            }
            Icon::Sleet | Icon::Hail => {
                self.cloud(cx, cy, DARK);
                drops(self, 2, RAIN);
                flakes(self, WHITE);
            }
            Icon::Storm => {
                self.cloud(cx, cy, DARK);
                let bolt = [(cx + 2, cy + 5), (cx - 1, cy + 9), (cx + 2, cy + 9), (cx - 1, cy + 13)];
                for w in bolt.windows(2) {
                    self.line(w[0], w[1], BOLT);
                    self.line((w[0].0 + 1, w[0].1), (w[1].0 + 1, w[1].1), BOLT);
                }
            }
            Icon::Fog => {
                for (i, y) in [cy - 4, cy - 1, cy + 2, cy + 5, cy + 8].into_iter().enumerate() {
                    let shift = if i % 2 == 0 { 0 } else { 3 };
                    self.line((cx - 9 + shift, y), (cx + 6 + shift, y), CLOUD);
                }
            }
            Icon::Wind => {
                const AIR: [u8; 3] = [170, 210, 230];
                for (y, x0, x1) in [(cy - 3, cx - 10, cx + 4), (cy + 2, cx - 7, cx + 9), (cy + 7, cx - 10, cx + 2)] {
                    self.line((x0, y), (x1, y), AIR);
                    self.line((x1 + 1, y - 1), (x1 + 2, y - 2), AIR);
                }
            }
            Icon::Unknown => {}
        }
    }
}

/// 3 × 5, bits left to right.
#[rustfmt::skip]
const SMALL: [(char, [u8; 5]); 58] = [
    ('0', [7, 5, 5, 5, 7]), ('1', [2, 6, 2, 2, 7]), ('2', [7, 1, 7, 4, 7]), ('3', [7, 1, 7, 1, 7]),
    ('4', [5, 5, 7, 1, 1]), ('5', [7, 4, 7, 1, 7]), ('6', [7, 4, 7, 5, 7]), ('7', [7, 1, 1, 2, 2]),
    ('8', [7, 5, 7, 5, 7]), ('9', [7, 5, 7, 1, 7]),
    ('A', [2, 5, 7, 5, 5]), ('B', [6, 5, 6, 5, 6]), ('C', [3, 4, 4, 4, 3]), ('D', [6, 5, 5, 5, 6]),
    ('E', [7, 4, 6, 4, 7]), ('F', [7, 4, 6, 4, 4]), ('G', [3, 4, 5, 5, 3]), ('H', [5, 5, 7, 5, 5]),
    ('I', [7, 2, 2, 2, 7]), ('J', [1, 1, 1, 5, 2]), ('K', [5, 5, 6, 5, 5]), ('L', [4, 4, 4, 4, 7]),
    ('M', [5, 7, 7, 5, 5]), ('N', [6, 5, 5, 5, 5]), ('O', [2, 5, 5, 5, 2]), ('P', [6, 5, 6, 4, 4]),
    ('Q', [2, 5, 5, 6, 3]), ('R', [6, 5, 6, 5, 5]), ('S', [3, 4, 2, 1, 6]), ('T', [7, 2, 2, 2, 2]),
    ('U', [5, 5, 5, 5, 7]), ('V', [5, 5, 5, 5, 2]), ('W', [5, 5, 5, 7, 5]), ('X', [5, 5, 2, 5, 5]),
    ('Y', [5, 5, 2, 2, 2]), ('Z', [7, 1, 2, 4, 7]),
    (' ', [0, 0, 0, 0, 0]), ('.', [0, 0, 0, 0, 2]), (',', [0, 0, 0, 2, 4]), (':', [0, 2, 0, 2, 0]),
    ('-', [0, 0, 7, 0, 0]), ('+', [0, 2, 7, 2, 0]), ('/', [1, 1, 2, 4, 4]), ('%', [5, 1, 2, 4, 5]),
    ('°', [2, 5, 2, 0, 0]), ('(', [2, 4, 4, 4, 2]), (')', [2, 1, 1, 1, 2]), ('!', [2, 2, 2, 0, 2]),
    ('?', [7, 1, 2, 0, 2]), ('\'', [2, 2, 0, 0, 0]), ('_', [0, 0, 0, 0, 7]), ('=', [0, 7, 0, 7, 0]),
    ('&', [2, 5, 2, 5, 3]), ('#', [5, 7, 5, 7, 5]), ('*', [5, 2, 7, 2, 5]), ('<', [1, 2, 4, 2, 1]),
    ('>', [4, 2, 1, 2, 4]), ('"', [5, 5, 0, 0, 0]),
];

/// 5 × 7, bits left to right.
#[rustfmt::skip]
const BIG: [(char, [u8; 7]); 44] = [
    ('0', [14, 17, 17, 17, 17, 17, 14]), ('1', [4, 12, 4, 4, 4, 4, 14]), ('2', [14, 17, 1, 2, 4, 8, 31]),
    ('3', [31, 2, 4, 2, 1, 17, 14]), ('4', [2, 6, 10, 18, 31, 2, 2]), ('5', [31, 16, 30, 1, 1, 17, 14]),
    ('6', [6, 8, 16, 30, 17, 17, 14]), ('7', [31, 1, 2, 4, 8, 8, 8]), ('8', [14, 17, 17, 14, 17, 17, 14]),
    ('9', [14, 17, 17, 15, 1, 2, 12]),
    ('A', [14, 17, 17, 31, 17, 17, 17]), ('B', [30, 17, 17, 30, 17, 17, 30]), ('C', [14, 17, 16, 16, 16, 17, 14]),
    ('D', [28, 18, 17, 17, 17, 18, 28]), ('E', [31, 16, 16, 30, 16, 16, 31]), ('F', [31, 16, 16, 30, 16, 16, 16]),
    ('G', [14, 17, 16, 23, 17, 17, 15]), ('H', [17, 17, 17, 31, 17, 17, 17]), ('I', [14, 4, 4, 4, 4, 4, 14]),
    ('J', [7, 2, 2, 2, 2, 18, 12]), ('K', [17, 18, 20, 24, 20, 18, 17]), ('L', [16, 16, 16, 16, 16, 16, 31]),
    ('M', [17, 27, 21, 21, 17, 17, 17]), ('N', [17, 17, 25, 21, 19, 17, 17]), ('O', [14, 17, 17, 17, 17, 17, 14]),
    ('P', [30, 17, 17, 30, 16, 16, 16]), ('Q', [14, 17, 17, 17, 21, 18, 13]), ('R', [30, 17, 17, 30, 20, 18, 17]),
    ('S', [15, 16, 16, 14, 1, 1, 30]), ('T', [31, 4, 4, 4, 4, 4, 4]), ('U', [17, 17, 17, 17, 17, 17, 14]),
    ('V', [17, 17, 17, 17, 17, 10, 4]), ('W', [17, 17, 17, 21, 21, 21, 10]), ('X', [17, 17, 10, 4, 10, 17, 17]),
    ('Y', [17, 17, 17, 10, 4, 4, 4]), ('Z', [31, 1, 2, 4, 8, 16, 31]),
    (':', [0, 0, 4, 0, 4, 0, 0]), ('.', [0, 0, 0, 0, 0, 0, 4]), ('-', [0, 0, 0, 14, 0, 0, 0]),
    ('°', [4, 10, 4, 0, 0, 0, 0]), ('%', [25, 25, 2, 4, 8, 19, 19]), ('+', [0, 4, 4, 31, 4, 4, 0]),
    ('/', [1, 1, 2, 4, 8, 16, 16]), (' ', [0, 0, 0, 0, 0, 0, 0]),
];

#[cfg(test)]
mod tests {
    use super::*;

    const ORANGE: [u8; 3] = [255, 138, 31];
    use crate::settings::IdleEntity;
    use serde_json::json;

    fn idle(show: IdleShow) -> Idle {
        Idle { show, ..Idle::default() }
    }

    fn px(rgb: &Rgb, x: usize, y: usize) -> [u8; 3] {
        rgb[(y * SIZE + x) * 3..][..3].try_into().unwrap()
    }

    fn lit_bar(rgb: &Rgb) -> Vec<i32> {
        let (left, right, top) = BAR;
        (left..right).filter(|&x| px(rgb, x as usize, top as usize) == PINK).collect()
    }

    #[test]
    fn update_fills_with_the_download() {
        let (left, right, top) = BAR;
        let lit = |rgb: &Rgb, c: [u8; 3]| (left..right).filter(|&x| px(rgb, x as usize, top as usize) == c).count();
        assert_eq!(lit_bar(&update(Update::Starting, 0)), (14..24).collect::<Vec<_>>()); // sweeps like start-up
        assert_eq!(lit(&update(Update::Downloading(0), 0), PINK), 0);
        assert_eq!(lit(&update(Update::Downloading(50), 0), PINK), 18);
        assert_eq!(lit(&update(Update::Downloading(100), 0), PINK), 36);
        assert_eq!(lit(&update(Update::Restarting, 0), PINK), 36);
        assert_eq!(lit(&update(Update::Failed, 0), RED), 36);
        // The percentage is written out, so 41% and 42% differ.
        assert_ne!(update(Update::Downloading(41), 0), update(Update::Downloading(42), 0));
    }

    #[test]
    fn boot_sweeps_until_ready() {
        let start = boot(Boot::WiFi, 0);
        assert_eq!(lit_bar(&start), (14..24).collect::<Vec<_>>());
        let half = boot(Boot::WiFi, SWEEP_MS / 2);
        assert_eq!(lit_bar(&half), (40..50).collect::<Vec<_>>()); // the far end
        assert_eq!(lit_bar(&boot(Boot::WiFi, SWEEP_MS)), lit_bar(&start)); // and back
        assert_eq!(lit_bar(&boot(Boot::Ready, 123)).len(), 36); // all of it
        // The wordmark: COVER pink, USSI white, all on the panel.
        let lit = |c: [u8; 3]| (0..SIZE).flat_map(|y| (0..SIZE).map(move |x| (x, y))).filter(|&(x, y)| px(&start, x, y) == c).count();
        assert!(lit(PINK) > 40 && lit(WHITE) > 30);
        assert_ne!(boot(Boot::WiFi, 0), boot(Boot::HomeAssistant, 0)); // says what it waits for
    }

    #[test]
    fn record_turns() {
        let a = record(0, 1800, PINK, 0.5);
        assert_eq!(px(&a, 0, 0), [0, 0, 0]); // the corners are off the record
        assert_eq!(px(&a, 31, 31), [0, 0, 0]); // the spindle hole
        assert_eq!(px(&a, 31, 24), PINK); // the label
        assert_eq!(px(&record(0, 1800, [200, 0, 0], 0.5), 31, 24), [200, 0, 0]);
        assert!(px(&a, 31, 4)[0] > 0); // the vinyl
        assert_ne!(record(450, 1800, PINK, 0.5), a);
        assert_eq!(record(1800, 1800, PINK, 0.5), a); // a whole turn
        assert_eq!(record(769, 769, PINK, 0.5), a); // at 78
        // Louder, a brighter sheen.
        let lit = |img: &Rgb| img.iter().map(|&v| v as u32).sum::<u32>();
        assert!(lit(&record(0, 1800, PINK, 1.0)) > lit(&record(0, 1800, PINK, 0.0)));
    }

    #[test]
    fn song_wraps_to_the_panel() {
        assert_eq!(wrap("With You (feat. PARTYNEXTDOOR)", Font::Big, 3), ["With You", "(feat.", "PARTYNEXTD"]);
        assert_eq!(wrap("I Don’t Wanna Know", Font::Small, 2), ["I Don't Wanna", "Know"]);
        let long = wrap("One Two Three Four Five Six Seven Eight Nine Ten", Font::Big, 3);
        assert!(long.len() == 3 && long[2].ends_with("..."), "{long:?}");
        assert!(long.iter().all(|l| Font::Big.width(l, 1) <= 60));
        let lit = |img: &Rgb| img.chunks(3).filter(|p| p.iter().any(|&v| v > 0)).count();
        assert!(lit(&song("Dreams", "Fleetwood Mac")) > 60);
        assert_eq!(song("With You (feat. PARTYNEXTDOOR)", "Drake"), song("With You", "Drake"));
        assert_eq!(lit(&song("", "")), 0);
    }

    const TUESDAY: LocalTime = LocalTime { minute: 14 * 60 + 5, weekday: 2, day: 3, month: 11, unix: 0 };

    #[test]
    fn clock_and_date() {
        let ha = HaStates::default();
        assert_eq!(content(&idle(IdleShow::Off), &ha, Some(TUESDAY)), None);
        let c = content(&idle(IdleShow::Clock), &ha, Some(TUESDAY)).unwrap();
        assert_eq!(c, Content::Clock { time: "14:05".into(), ampm: String::new(), date: "TUE 3 NOV".into() });
        let twelve = Idle { clock_24h: false, ..idle(IdleShow::Clock) };
        let c = content(&twelve, &ha, Some(LocalTime { minute: 5, ..TUESDAY })).unwrap();
        assert!(matches!(c, Content::Clock { ref time, ref ampm, .. } if time == "12:05" && ampm == "AM"));
        let c = content(&idle(IdleShow::Clock), &ha, None).unwrap();
        assert_eq!(c, Content::Clock { time: "--:--".into(), ampm: String::new(), date: "".into() });
    }

    #[test]
    fn weather_from_home_assistant() {
        let mut ha = HaStates::default();
        let s = Idle { weather: "weather.home".into(), ..idle(IdleShow::Weather) };
        let c = content(&s, &ha, Some(TUESDAY)).unwrap();
        assert!(matches!(c, Content::Weather { icon: Icon::Unknown, ref temperature, .. } if temperature.is_empty()));
        ha.apply(&json!({"a": {"weather.home": {"s": "partlycloudy", "a": {"temperature": 11.6, "temperature_unit": "°C"}}}}));
        let c = content(&s, &ha, Some(TUESDAY)).unwrap();
        assert_eq!(c, Content::Weather {
            icon: Icon::PartlyCloudy,
            temperature: "12°".into(),
            condition: "PARTLY CLOUDY".into(),
            time: "14:05".into(),
            ampm: String::new(),
        });
    }

    #[test]
    fn entities_with_units() {
        let mut ha = HaStates::default();
        ha.apply(&json!({"a": {
            "sensor.outside": {"s": "11.437", "a": {"friendly_name": "Outside", "unit_of_measurement": "°C"}},
            "sensor.power": {"s": "1.2", "a": {"friendly_name": "Power", "unit_of_measurement": "kWh"}},
            "light.desk": {"s": "on", "a": {"friendly_name": "Desk lamp"}},
        }}));
        let entity = |e: &str, name: &str| IdleEntity { entity: e.into(), name: name.into() };
        let s = Idle {
            entities: vec![entity("sensor.outside", ""), entity("sensor.power", "Today"), entity("light.desk", ""), entity("sensor.gone", "")],
            ..idle(IdleShow::Entities)
        };
        let Some(Content::Entities(rows)) = content(&s, &ha, None) else { panic!() };
        let rows: Vec<(&str, &str)> = rows.iter().map(|(a, b)| (a.as_str(), b.as_str())).collect();
        assert_eq!(rows, [("OUTSIDE", "11.4°C"), ("TODAY", "1.2 KWH"), ("DESK LAMP", "ON"), ("SENSOR.GONE", "--")]);
        assert_eq!(entity_value("unavailable", "%"), "--");
        assert_eq!(entity_value("45", "%"), "45%");
    }

    #[test]
    fn draws_inside_the_panel() {
        let lit = |img: &Rgb| img.chunks(3).filter(|p| p.iter().any(|&v| v > 0)).count();
        let clock = draw(&Content::Clock { time: "23:59".into(), ampm: String::new(), date: "WED 30 SEP".into() });
        assert!(lit(&clock) > 100);
        // The time is centred: as much dark on each side.
        let column = |img: &Rgb, x: usize| (0..SIZE).any(|y| img[(y * SIZE + x) * 3] > 0);
        let first = (0..SIZE).find(|&x| column(&clock, x)).unwrap();
        let last = (0..SIZE).rev().find(|&x| column(&clock, x)).unwrap();
        assert!((first as i32 - (SIZE - 1 - last) as i32).abs() <= 1, "{first} {last}");
        for icon in [Icon::Sun, Icon::Moon, Icon::PartlyCloudy, Icon::Cloud, Icon::Rain, Icon::Pouring, Icon::Snow,
            Icon::Sleet, Icon::Hail, Icon::Storm, Icon::Fog, Icon::Wind] {
            let w = draw(&Content::Weather { icon, temperature: "-12°".into(), condition: "PARTLY CLOUDY".into(), time: "12:00".into(), ampm: String::new() });
            assert!(lit(&w) > lit(&draw(&Content::Weather {
                icon: Icon::Unknown, temperature: "-12°".into(), condition: "PARTLY CLOUDY".into(), time: "12:00".into(), ampm: String::new()
            })), "{icon:?}");
        }
        let cut = fit("A VERY LONG ENTITY NAME INDEED", Font::Small, 1);
        assert!(cut.starts_with("A VERY LONG") && Font::Small.width(&cut, 1) <= 60 && Font::Small.width(&cut, 1) > 55, "{cut}");
        assert_eq!(Font::Big.width("1", 1), 5);
        assert_eq!(Font::Big.width("I", 1), 3);
    }

    #[test]
    fn the_ring_runs_down_and_fills_up_again() {
        let mut r = Ring::default();
        assert_eq!(r.lit(200, 0, false), 200); // coming up: as it is
        assert_eq!(r.lit(201, 100, true), 201);
        // The next video, at the start: down from 201 to nothing, then up to it.
        let down: Vec<u32> = (0..=6).map(|i| r.lit(0, 1000 + i * 100, true)).collect();
        assert!(down.windows(2).all(|w| w[1] <= w[0]) && down[0] == 201 && down[6] == 0, "{down:?}");
        let up: Vec<u32> = (1..=8).map(|i| r.lit(40, 1600 + i * 100, true)).collect();
        assert!(up.windows(2).all(|w| w[1] >= w[0]) && up[7] == 40, "{up:?}");
        assert_eq!(r.lit(41, 3000, true), 41);
        // Off the progress screen and back: no run down.
        assert_eq!(r.lit(5, 3100, false), 5);
    }

    #[test]
    fn progress_screen() {
        let ha = HaStates::default();
        let clock = LocalTime { minute: 21 * 60 + 4, unix: 10_090, ..Default::default() };
        let mut p = Player { state: "playing".into(), position: Some(4320.0), position_at: Some(10_000.0), ..Default::default() };
        p.duration = Some(8880.0);
        // The idle screen inside the ring; the clock when the idle screen is off.
        let clock_screen = content(&idle(IdleShow::Clock), &ha, Some(clock)).unwrap();
        let Content::Progress { screen, lit, .. } = progress(&idle(IdleShow::Off), &ha, Some(&p), Some(clock), ORANGE) else { panic!() };
        assert_eq!((*screen, lit), (clock_screen.clone(), 125));
        let Content::Progress { screen, .. } = progress(&idle(IdleShow::Weather), &ha, Some(&p), Some(clock), ORANGE) else { panic!() };
        assert!(matches!(*screen, Content::Weather { .. }));
        // No length: no ring.
        p.duration = None;
        assert!(matches!(progress(&idle(IdleShow::Clock), &ha, Some(&p), Some(clock), ORANGE), Content::Progress { lit: 0, .. }));
        // The ring goes once round the border, clockwise from the top middle.
        let all: std::collections::HashSet<_> = (0..RING).map(ring).collect();
        assert_eq!((all.len(), RING), (252, 252));
        assert_eq!((ring(0), ring(1), ring(31), ring(32), ring(94), ring(251)), ((32, 0), (33, 0), (63, 0), (63, 1), (63, 63), (31, 0)));
        assert!(all.iter().all(|&(x, y)| x == 0 || y == 0 || x == 63 || y == 63));
        let img = draw(&Content::Progress { screen: Box::new(clock_screen), lit: 2, color: ORANGE });
        let px = |x: usize, y: usize| [img[(y * SIZE + x) * 3], img[(y * SIZE + x) * 3 + 1], img[(y * SIZE + x) * 3 + 2]];
        assert_eq!((px(32, 0), px(33, 0), px(34, 0)), (ORANGE, ORANGE, DIM));
        assert!((19..33).any(|y| (8..56).any(|x| px(x, y) == WHITE))); // the clock
    }
}
