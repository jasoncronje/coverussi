//! What the panel shows when there's no art: a clock, the weather or a few
//! Home Assistant entities, drawn in two pixel fonts. Also the start-up screen.

use serde_json::Value;

use crate::art::{Rgb, SIZE};
use crate::ha::HaStates;
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
}

/// The most entities the screen fits.
pub const MAX_ENTITIES: usize = 4;

const WHITE: [u8; 3] = [255, 255, 255];
const GRAY: [u8; 3] = [150, 150, 150];
/// The wordmark's amber (scripts/logo.py).
const AMBER: [u8; 3] = [242, 179, 61];
/// An unlit LED in the wordmark's style.
const UNLIT: [u8; 3] = [38, 38, 38];

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
    Clock { time: String, date: String },
    Weather { icon: Icon, temperature: String, condition: String, time: String },
    /// (label, value) rows.
    Entities(Vec<(String, String)>),
}

/// The screen to show, or None to leave the panel dark.
pub fn content(idle: &Idle, ha: &HaStates, clock: Option<LocalTime>) -> Option<Content> {
    let time = || clock.map_or("--:--".to_string(), |c| clock_text(c.minute, idle.clock_24h));
    match idle.show {
        IdleShow::Off => None,
        IdleShow::Clock => Some(Content::Clock {
            time: time(),
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
    let mut p = Canvas(Box::new([0; SIZE * SIZE * 3]));
    match c {
        Content::Clock { time, date } => {
            p.centered(time, Font::Big, 2, 19, WHITE);
            p.centered(date, Font::Small, 1, 39, GRAY);
        }
        Content::Weather { icon, temperature, condition, time } => {
            p.icon(*icon, 15, 13);
            let scale = if Font::Big.width(temperature, 2) <= 32 { 2 } else { 1 };
            let w = Font::Big.width(temperature, scale);
            p.text(temperature, Font::Big, scale, 62 - w, if scale == 2 { 7 } else { 10 }, WHITE);
            p.centered(condition, Font::Small, 1, 31, GRAY);
            p.centered(time, Font::Big, 2, 43, WHITE);
        }
        Content::Entities(rows) => {
            let top = (SIZE as i32 - rows.len() as i32 * 16 + 2) / 2;
            for (i, (label, value)) in rows.iter().enumerate() {
                let y = top + i as i32 * 16;
                p.text(&fit(label, Font::Small, 1), Font::Small, 1, 2, y, GRAY);
                p.text(&fit(value, Font::Big, 1), Font::Big, 1, 2, y + 7, WHITE);
            }
        }
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
    let mut p = Canvas(Box::new([0; SIZE * SIZE * 3]));
    let x = (SIZE as i32 - Font::Big.width("COVERUSSI", 1)) / 2;
    p.text("COVER", Font::Big, 1, x, 18, AMBER);
    p.text("USSI", Font::Big, 1, x + Font::Big.width("COVER", 1) + 1, 18, WHITE);
    let (left, right, top) = BAR;
    let lit = if stage == Boot::Ready {
        left..right
    } else {
        // A 10-pixel segment, back and forth.
        let span = (right - left - 10) as u64;
        let phase = t % SWEEP_MS * 2 * span / SWEEP_MS;
        let at = left + if phase <= span { phase } else { 2 * span - phase } as i32;
        at..at + 10
    };
    for x in left..right {
        for y in top..top + 2 {
            p.set(x, y, if lit.contains(&x) { AMBER } else { UNLIT });
        }
    }
    let label = match stage {
        Boot::WiFi => "WI-FI",
        Boot::HomeAssistant => "HOME ASSISTANT",
        Boot::Ready => "READY",
    };
    p.centered(label, Font::Small, 1, 39, GRAY);
    p.0
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
            // A small glyph in the big font sits on the baseline.
            let top = y + (font.height() - height) as i32 * scale;
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
        (left..right).filter(|&x| px(rgb, x as usize, top as usize) == AMBER).collect()
    }

    #[test]
    fn boot_sweeps_until_ready() {
        let start = boot(Boot::WiFi, 0);
        assert_eq!(lit_bar(&start), (14..24).collect::<Vec<_>>());
        let half = boot(Boot::WiFi, SWEEP_MS / 2);
        assert_eq!(lit_bar(&half), (40..50).collect::<Vec<_>>()); // the far end
        assert_eq!(lit_bar(&boot(Boot::WiFi, SWEEP_MS)), lit_bar(&start)); // and back
        assert_eq!(lit_bar(&boot(Boot::Ready, 123)).len(), 36); // all of it
        // The wordmark: COVER amber, USSI white, all on the panel.
        let lit = |c: [u8; 3]| (0..SIZE).flat_map(|y| (0..SIZE).map(move |x| (x, y))).filter(|&(x, y)| px(&start, x, y) == c).count();
        assert!(lit(AMBER) > 40 && lit(WHITE) > 30);
        assert_ne!(boot(Boot::WiFi, 0), boot(Boot::HomeAssistant, 0)); // says what it waits for
    }

    const TUESDAY: LocalTime = LocalTime { minute: 14 * 60 + 5, weekday: 2, day: 3, month: 11 };

    #[test]
    fn clock_and_date() {
        let ha = HaStates::default();
        assert_eq!(content(&idle(IdleShow::Off), &ha, Some(TUESDAY)), None);
        let c = content(&idle(IdleShow::Clock), &ha, Some(TUESDAY)).unwrap();
        assert_eq!(c, Content::Clock { time: "14:05".into(), date: "TUE 3 NOV".into() });
        let twelve = Idle { clock_24h: false, ..idle(IdleShow::Clock) };
        let c = content(&twelve, &ha, Some(LocalTime { minute: 5, ..TUESDAY })).unwrap();
        assert!(matches!(c, Content::Clock { ref time, .. } if time == "12:05"));
        let c = content(&idle(IdleShow::Clock), &ha, None).unwrap();
        assert_eq!(c, Content::Clock { time: "--:--".into(), date: "".into() });
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
            time: "14:05".into()
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
        let clock = draw(&Content::Clock { time: "23:59".into(), date: "WED 30 SEP".into() });
        assert!(lit(&clock) > 100);
        // The time is centred: as much dark on each side.
        let column = |img: &Rgb, x: usize| (0..SIZE).any(|y| img[(y * SIZE + x) * 3] > 0);
        let first = (0..SIZE).find(|&x| column(&clock, x)).unwrap();
        let last = (0..SIZE).rev().find(|&x| column(&clock, x)).unwrap();
        assert!((first as i32 - (SIZE - 1 - last) as i32).abs() <= 1, "{first} {last}");
        for icon in [Icon::Sun, Icon::Moon, Icon::PartlyCloudy, Icon::Cloud, Icon::Rain, Icon::Pouring, Icon::Snow,
            Icon::Sleet, Icon::Hail, Icon::Storm, Icon::Fog, Icon::Wind] {
            let w = draw(&Content::Weather { icon, temperature: "-12°".into(), condition: "PARTLY CLOUDY".into(), time: "12:00".into() });
            assert!(lit(&w) > lit(&draw(&Content::Weather {
                icon: Icon::Unknown, temperature: "-12°".into(), condition: "PARTLY CLOUDY".into(), time: "12:00".into()
            })), "{icon:?}");
        }
        let cut = fit("A VERY LONG ENTITY NAME INDEED", Font::Small, 1);
        assert!(cut.starts_with("A VERY LONG") && Font::Small.width(&cut, 1) <= 60 && Font::Small.width(&cut, 1) > 55, "{cut}");
        assert_eq!(Font::Big.width("1", 1), 5);
        assert_eq!(Font::Big.width("I", 1), 3);
    }
}
