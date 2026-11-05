//! From decoded artwork to the 64 × 64 frame: crop to square, area-average,
//! transition, dim, and keep the panel's current under the limit.

use std::sync::{Arc, OnceLock};

use crate::settings::{Color, Fit, Transition};

pub const SIZE: usize = 64;
const PIXELS: usize = SIZE * SIZE;

/// What the art becomes: 64 × 64 RGB888.
pub type Rgb = [u8; PIXELS * 3];
/// RGB565, as drawn and mirrored.
pub type Frame = [u16; PIXELS];

fn rgb565(r: f32, g: f32, b: f32) -> u16 {
    let c = |v: f32| v.clamp(0.0, 255.0) as u16;
    ((c(r) & 0xF8) << 8) | ((c(g) & 0xFC) << 3) | (c(b) >> 3)
}

/// RGB565 back to 8 bits a channel, full scale: 0x1F becomes 255, not 248.
fn expand(p: u16) -> [u8; 3] {
    let (r5, g6, b5) = (p >> 11, (p >> 5) & 0x3F, p & 0x1F);
    [((r5 << 3) | (r5 >> 2)) as u8, ((g6 << 2) | (g6 >> 4)) as u8, ((b5 << 3) | (b5 >> 2)) as u8]
}

/// CIE 1931 lightness to linear light, 0-1, as the HUB75 driver corrects it.
fn cie() -> &'static [f32; 256] {
    static TABLE: OnceLock<[f32; 256]> = OnceLock::new();
    TABLE.get_or_init(|| {
        std::array::from_fn(|i| {
            let l = i as f32 * 100.0 / 255.0;
            if l <= 8.0 { l / 903.3 } else { ((l + 16.0) / 116.0).powi(3) }
        })
    })
}

fn smooth(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// A fixed pseudo-random order for the dissolve, 0-1 a pixel (the portal's preview uses the same hash).
fn dissolve_rank(i: usize) -> f32 {
    let mut h = i as u32;
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb352d);
    h ^= h >> 15;
    h = h.wrapping_mul(0x846ca68b);
    h ^= h >> 16;
    (h & 0xFFFF) as f32 / 65536.0
}

/// The picture inside black bars baked into one that isn't square, like
/// YouTube's 4:3 thumbnails of 16:9 videos: (x, y, w, h). Square art is left
/// alone, so a dark cover keeps its edges; at most a quarter goes from each side.
fn inside_bars(rgb: &[u8], w: usize, h: usize) -> (usize, usize, usize, usize) {
    if w == h {
        return (0, 0, w, h);
    }
    // JPEG noise keeps black from being exactly 0.
    let dark = |x: usize, y: usize| rgb[(y * w + x) * 3..][..3].iter().all(|&v| v <= 24);
    let (mut top, mut bottom, mut left, mut right) = (0, h, 0, w);
    while top < h / 4 && (0..w).all(|x| dark(x, top)) {
        top += 1;
    }
    while h - bottom < h / 4 && (0..w).all(|x| dark(x, bottom - 1)) {
        bottom -= 1;
    }
    while left < w / 4 && (top..bottom).all(|y| dark(left, y)) {
        left += 1;
    }
    while w - right < w / 4 && (top..bottom).all(|y| dark(right - 1, y)) {
        right -= 1;
    }
    (left, top, right - left, bottom - top)
}

/// Average the `src` rectangle (x, y, w, h) of `rgb` (`w` wide) into the `dst`
/// rectangle of `out` (`out_w` wide): each output pixel averages the source pixels it covers.
fn average(rgb: &[u8], w: usize, src: (usize, usize, usize, usize), out: &mut [u8], out_w: usize, dst: (usize, usize, usize, usize)) {
    let ((x0, y0, sw, sh), (dx, dy, dw, dh)) = (src, dst);
    for oy in 0..dh {
        let sy0 = y0 + oy * sh / dh;
        let sy1 = (sy0 + 1).max(y0 + (oy + 1) * sh / dh);
        for ox in 0..dw {
            let sx0 = x0 + ox * sw / dw;
            let sx1 = (sx0 + 1).max(x0 + (ox + 1) * sw / dw);
            let mut sum = [0u32; 3];
            let mut n = 0;
            for sy in sy0..sy1 {
                for sx in sx0..sx1 {
                    let p = (sy * w + sx) * 3;
                    for k in 0..3 {
                        sum[k] += rgb[p + k] as u32;
                    }
                    n += 1;
                }
            }
            let o = ((dy + oy) * out_w + dx + ox) * 3;
            for k in 0..3 {
                out[o + k] = (sum[k] / n) as u8;
            }
        }
    }
}

/// What a pan travels across: the wide picture inside its bars, at most three squares wide.
fn pan_region(rgb: &[u8], w: usize, h: usize) -> Option<(usize, usize, usize, usize)> {
    let (bx, by, bw, bh) = inside_bars(rgb, w, h);
    let sw = bw.min(bh * 3);
    (sw * SIZE / bh > SIZE).then_some((bx + (bw - sw) / 2, by, sw, bh))
}

/// Average `rgb` (w × h, 3 bytes a pixel) down to 64 × 64. A picture that isn't
/// square loses any black bars it came with, then is center-cropped to a square,
/// or shown whole between black bars of the panel's own. A picture to pan
/// across is cropped here; its strip (`scale_to_strip`) gives the left end.
pub fn scale_to_square(rgb: &[u8], w: usize, h: usize, fit: Fit) -> Box<Rgb> {
    let mut out = Box::new([0u8; PIXELS * 3]);
    let (bx, by, bw, bh) = inside_bars(rgb, w, h);
    // The part of the source that's drawn, and where on the panel it goes.
    let (src, dst) = match fit {
        Fit::Crop | Fit::Pan => {
            let side = bw.min(bh);
            ((bx + (bw - side) / 2, by + (bh - side) / 2, side, side), (0, 0, SIZE, SIZE))
        }
        Fit::Letterbox => {
            let (dw, dh) = if bw >= bh {
                (SIZE, ((SIZE * bh + bw / 2) / bw).max(1))
            } else {
                (((SIZE * bw + bh / 2) / bh).max(1), SIZE)
            };
            ((bx, by, bw, bh), ((SIZE - dw) / 2, (SIZE - dh) / 2, dw, dh))
        }
    };
    average(rgb, w, src, &mut out[..], SIZE, dst);
    out
}

/// A wide picture at the panel's height, for panning across.
pub struct Strip {
    pub w: usize,
    pub rgb: Vec<u8>,
}

impl Strip {
    /// Its left end, where a pan starts.
    pub fn left(&self) -> Box<Rgb> {
        let mut out = Box::new([0u8; PIXELS * 3]);
        for (y, row) in out.chunks_exact_mut(SIZE * 3).enumerate() {
            row.copy_from_slice(&self.rgb[y * self.w * 3..][..SIZE * 3]);
        }
        out
    }
}

/// The strip to pan across, or None when the picture isn't wider than square.
pub fn scale_to_strip(rgb: &[u8], w: usize, h: usize) -> Option<Strip> {
    let (x0, y0, sw, sh) = pan_region(rgb, w, h)?;
    let out_w = (sw * SIZE + sh / 2) / sh;
    let mut out = vec![0u8; out_w * SIZE * 3];
    average(rgb, w, (x0, y0, sw, sh), &mut out, out_w, (0, 0, out_w, SIZE));
    Some(Strip { w: out_w, rgb: out })
}

/// A pan rests at each end this long.
const PAN_HOLD_MS: u64 = 4000;
/// And moves a pixel this often, about 5 a second.
const PAN_MS_PER_PX: u64 = 200;

/// How far into the strip the panel's left edge is, `ms` after the pan starts:
/// rest at the left, ease to the right end, rest, ease back, over and over.
fn pan_offset(ms: u64, travel: usize) -> f32 {
    let move_ms = travel as u64 * PAN_MS_PER_PX;
    let leg = PAN_HOLD_MS + move_ms;
    let t = ms % (2 * leg);
    let (into, back) = if t < leg { (t, false) } else { (t - leg, true) };
    let e = smooth(into.saturating_sub(PAN_HOLD_MS) as f32 / move_ms.max(1) as f32);
    travel as f32 * if back { 1.0 - e } else { e }
}

/// Primaries, a gray ramp, a hue sweep and corner markers (red top-left,
/// green top-right, blue bottom-left, white bottom-right).
pub fn test_pattern() -> Box<Rgb> {
    let mut out = Box::new([0u8; PIXELS * 3]);
    let mut set = |x: usize, y: usize, c: [u8; 3]| out[(y * SIZE + x) * 3..][..3].copy_from_slice(&c);
    for y in 0..SIZE {
        for x in 0..SIZE {
            if y < 16 {
                let band = x / 16;
                let on = |b: bool| if b { 255 } else { 0 };
                set(x, y, [on(band == 0 || band == 3), on(band == 1 || band == 3), on(band >= 2)]);
            } else if y < 32 {
                let v = (x * 255 / (SIZE - 1)) as u8;
                set(x, y, [v, v, v]);
            } else if y < 48 {
                let h = x as f32 * 6.0 / SIZE as f32;
                let f = h - h.floor();
                let (up, down) = ((255.0 * f) as u8, (255.0 * (1.0 - f)) as u8);
                let rgb = [[255, up, 0], [down, 255, 0], [0, 255, up], [0, down, 255], [up, 0, 255], [255, 0, down]];
                set(x, y, rgb[(h as usize).min(5)]);
            }
        }
    }
    for (cx, cy, c) in [(0, 0, [255, 0, 0]), (60, 0, [0, 255, 0]), (0, 60, [0, 0, 255]), (60, 60, [255, 255, 255])] {
        for dy in 0..4 {
            for dx in 0..4 {
                set(cx + dx, cy + dy, c);
            }
        }
    }
    out
}

/// For the calibration screen: white beside mid gray (white balance), eight
/// near-black steps (shadows), eight colours with a skin tone (saturation) and
/// a gray ramp, top to bottom.
pub const CALIBRATION_SHADOW_STEPS: [u8; 8] = [0, 4, 8, 14, 22, 34, 52, 80];
pub const CALIBRATION_COLOURS: [[u8; 3]; 8] =
    [[255, 0, 0], [255, 128, 0], [255, 255, 0], [0, 255, 0], [0, 255, 255], [0, 0, 255], [255, 0, 255], [230, 170, 130]];

pub fn calibration_image() -> Box<Rgb> {
    let mut out = Box::new([0u8; PIXELS * 3]);
    for y in 0..SIZE {
        for x in 0..SIZE {
            let c = match y {
                0..20 if x < 32 => [255; 3],
                0..20 => [128; 3],
                20..36 => [CALIBRATION_SHADOW_STEPS[x / 8]; 3],
                36..52 => CALIBRATION_COLOURS[x / 8],
                _ => [(x * 255 / (SIZE - 1)) as u8; 3],
            };
            out[(y * SIZE + x) * 3..][..3].copy_from_slice(&c);
        }
    }
    out
}

/// A colour tuning as lookup tables, ready to apply to each frame: saturation,
/// then shadows and strength per channel. The web app's mirror keeps the untouched frame.
pub struct Grade {
    saturation: i32,
    curves: [[u8; 256]; 3],
}

impl Grade {
    /// None when the tuning changes nothing.
    pub fn new(c: &Color) -> Option<Self> {
        if *c == Color::default() {
            return None;
        }
        // 2^(-shadows/75): +50 is an exponent of 0.63, -50 of 1.59.
        let exponent = 2f32.powf(-c.shadows as f32 / 75.0);
        let curve = |pct: i64| -> [u8; 256] {
            std::array::from_fn(|i| ((i as f32 / 255.0).powf(exponent) * 255.0 * pct as f32 / 100.0).round() as u8)
        };
        Some(Self { saturation: c.saturation_pct as i32, curves: [curve(c.red_pct), curve(c.green_pct), curve(c.blue_pct)] })
    }

    pub fn apply(&self, f: &Frame, out: &mut Frame) {
        for (o, &p) in out.iter_mut().zip(f.iter()) {
            let [r, g, b] = expand(p).map(i32::from);
            let luma = (54 * r + 183 * g + 19 * b) >> 8;
            let c = [r, g, b].map(|v| (luma + (v - luma) * self.saturation / 100).clamp(0, 255) as usize);
            let [r, g, b] = [0, 1, 2].map(|k| self.curves[k][c[k]] as f32);
            *o = rgb565(r, g, b);
        }
    }
}

/// The frame turned clockwise by 90, 180 or 270 degrees (anything else copies it).
pub fn rotate(f: &Frame, degrees: i64, out: &mut Frame) {
    let n = SIZE - 1;
    for y in 0..SIZE {
        for x in 0..SIZE {
            // Where the pixel now at (x, y) came from.
            let (sx, sy) = match degrees {
                90 => (y, n - x),
                180 => (n - x, n - y),
                270 => (n - y, x),
                _ => (x, y),
            };
            out[y * SIZE + x] = f[sy * SIZE + sx];
        }
    }
}

/// Estimated panel current with nothing lit, and with all of it white at full brightness.
/// FullWhite is a placeholder until an inline meter measures it.
pub const IDLE_MA: i64 = 150;
pub const FULL_WHITE_MA: i64 = 4000;

/// Estimated panel current for a frame at a brightness, in mA. The HUB75
/// driver applies CIE 1931 gamma, so a channel's current follows its corrected value.
pub fn estimate_ma(f: &Frame, brightness: i64) -> i64 {
    let t = cie();
    let lit: f32 = f
        .iter()
        .map(|&p| {
            let [r, g, b] = expand(p);
            (t[r as usize] + t[g as usize] + t[b as usize]) / 3.0
        })
        .sum();
    let share = lit / PIXELS as f32 * (brightness as f32 / 255.0);
    IDLE_MA + (share * (FULL_WHITE_MA - IDLE_MA) as f32) as i64
}

/// The brightness (≤ requested) that keeps the estimate within `limit_ma`.
pub fn limited_brightness(f: &Frame, brightness: i64, limit_ma: i64) -> i64 {
    let at = estimate_ma(f, brightness);
    if at <= limit_ma || brightness == 0 {
        return brightness;
    }
    // Current above idle scales with brightness.
    let scale = (limit_ma - IDLE_MA) as f32 / (at - IDLE_MA) as f32;
    ((brightness as f32 * scale) as i64).clamp(0, brightness)
}

/// Moves between pictures. Show a new one (or none) and render every frame.
pub struct Compositor {
    from: Option<Arc<Rgb>>,
    to: Option<Arc<Rgb>>,
    key: String,
    start: u64,
    fade_ms: u64,
    how: Transition,
    dim: bool,
    shown_dim: f32,
    last: Box<Frame>,
    rendered: bool,
    /// The wide picture `to` is the left end of, panned across once it's in.
    pan: Option<Arc<Strip>>,
}

impl Default for Compositor {
    fn default() -> Self {
        Self {
            from: None,
            to: None,
            key: String::new(),
            start: 0,
            fade_ms: 0,
            how: Transition::Crossfade,
            dim: false,
            shown_dim: 1.0,
            last: Box::new([0; PIXELS]),
            rendered: false,
            pan: None,
        }
    }
}

impl Compositor {
    pub fn show(&mut self, art: Option<Arc<Rgb>>, key: &str, now: u64, fade_ms: u64, how: Transition) {
        if key == self.key {
            return;
        }
        // Start from what's on screen now, mid-fade included.
        self.from = self.rendered.then(|| {
            let mut snapshot = Box::new([0u8; PIXELS * 3]);
            for (i, &p) in self.last.iter().enumerate() {
                snapshot[i * 3..][..3].copy_from_slice(&expand(p));
            }
            Arc::from(snapshot)
        });
        if self.rendered {
            self.shown_dim = 1.0; // the snapshot already carries the dimming
        }
        self.to = art;
        self.pan = None;
        self.key = key.to_string();
        self.start = now;
        self.fade_ms = if how == Transition::Cut { 0 } else { fade_ms };
        self.how = how;
    }

    /// Pan across `strip` once the picture just shown is in.
    pub fn pan(&mut self, strip: Option<Arc<Strip>>) {
        self.pan = strip;
    }

    pub fn set_dim(&mut self, dim: bool) {
        self.dim = dim;
    }

    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn animating(&self, now: u64) -> bool {
        now < self.start + self.fade_ms
    }

    /// Render into `f`; true if it differs from the last render.
    pub fn render(&mut self, now: u64, f: &mut Frame) -> bool {
        let t = if self.fade_ms == 0 || now >= self.start + self.fade_ms {
            1.0
        } else {
            now.saturating_sub(self.start) as f32 / self.fade_ms as f32
        };
        let target_dim = if self.dim { 0.35 } else { 1.0 };
        // Dimming eases too, about as fast as a fade.
        self.shown_dim += (target_dim - self.shown_dim) * if self.fade_ms > 0 { 0.15 } else { 1.0 };
        if (self.shown_dim - target_dim).abs() < 0.01 {
            self.shown_dim = target_dim;
        }
        // No picture before or after is black.
        let (from_img, to_img, dim) = (self.from.as_deref(), self.to.as_deref(), self.shown_dim);
        let from = |x: usize, y: usize, k: usize| from_img.map_or(0.0, |p| p[(y * SIZE + x) * 3 + k] as f32);
        // Panning starts when the transition ends, from the left end `to` already shows.
        let pan = self.pan.as_deref().map(|s| (s, pan_offset(now.saturating_sub(self.start + self.fade_ms), s.w - SIZE)));
        let to = |x: usize, y: usize, k: usize| match pan {
            Some((s, off)) => {
                // Between two columns, blended, so the slide is smooth at 64 pixels.
                let fx = x as f32 + off;
                let (x0, f) = (fx as usize, fx.fract());
                let x1 = (x0 + 1).min(s.w - 1);
                let (a, b) = (s.rgb[(y * s.w + x0) * 3 + k] as f32, s.rgb[(y * s.w + x1) * 3 + k] as f32);
                (a + (b - a) * f) * dim
            }
            None => to_img.map_or(0.0, |p| p[(y * SIZE + x) * 3 + k] as f32 * dim),
        };
        let e = smooth(t);
        // Slide: the new picture pushes the old one off to the left.
        let shift = (e * SIZE as f32).round() as usize;
        let how = if t >= 1.0 { Transition::Cut } else { self.how };
        for y in 0..SIZE {
            for x in 0..SIZE {
                let i = y * SIZE + x;
                let mut c = [0f32; 3];
                for (k, c) in c.iter_mut().enumerate() {
                    let (a, b) = (from(x, y, k), to(x, y, k));
                    *c = match how {
                        Transition::Cut => b,
                        Transition::Crossfade => a + (b - a) * t,
                        Transition::FadeBlack => {
                            if t < 0.5 { a * (1.0 - 2.0 * t) } else { b * (2.0 * t - 1.0) }
                        }
                        Transition::Slide => {
                            if x + shift < SIZE { from(x + shift, y, k) } else { to(x + shift - SIZE, y, k) }
                        }
                        Transition::Wipe => {
                            // A soft edge four pixels wide sweeps left to right.
                            let w = ((e * (SIZE + 4) as f32 - x as f32) / 4.0).clamp(0.0, 1.0);
                            a + (b - a) * w
                        }
                        Transition::Dissolve => {
                            if dissolve_rank(i) < t { b } else { a }
                        }
                    };
                }
                f[i] = rgb565(c[0], c[1], c[2]);
            }
        }
        if t >= 1.0 {
            self.from = None;
        }
        let changed = !self.rendered || f[..] != self.last[..];
        self.last.copy_from_slice(f);
        self.rendered = true;
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rotation_turns_clockwise() {
        let n = SIZE - 1;
        let mut f = [0u16; PIXELS];
        f[0] = 1; // top left
        f[n] = 2; // top right
        let mut out = [0u16; PIXELS];
        rotate(&f, 90, &mut out);
        assert_eq!((out[n], out[n * SIZE + n]), (1, 2));
        rotate(&f, 180, &mut out);
        assert_eq!((out[n * SIZE + n], out[n * SIZE]), (1, 2));
        rotate(&f, 270, &mut out);
        assert_eq!((out[n * SIZE], out[0]), (1, 2));
        rotate(&f, 0, &mut out);
        assert_eq!(out, f);
    }

    fn solid(r: u8, g: u8, b: u8) -> Arc<Rgb> {
        let mut img = Box::new([0u8; PIXELS * 3]);
        for p in img.chunks_exact_mut(3) {
            p.copy_from_slice(&[r, g, b]);
        }
        Arc::from(img)
    }

    #[test]
    fn scale_to_square_crops_and_averages() {
        // 128 × 64: left half red, right half blue; the square crop is the middle 64.
        let mut img = vec![0u8; 128 * 64 * 3];
        for y in 0..64 {
            for x in 0..128 {
                img[(y * 128 + x) * 3 + if x < 64 { 0 } else { 2 }] = 255;
            }
        }
        let out = scale_to_square(&img, 128, 64, Fit::Crop);
        assert!(out[0] == 255 && out[2] == 0); // top-left: red (source x 32)
        assert!(out[63 * 3] == 0 && out[63 * 3 + 2] == 255); // top-right: blue
        // 256 × 256 checkerboard of 2 × 2 cells averages to mid gray.
        let mut check = vec![0u8; 256 * 256 * 3];
        for y in 0..256 {
            for x in 0..256 {
                let v = if (x / 2 + y / 2) % 2 == 1 { 255 } else { 0 };
                check[(y * 256 + x) * 3..][..3].fill(v);
            }
        }
        assert_eq!(scale_to_square(&check, 256, 256, Fit::Crop)[0], 127);
    }

    #[test]
    fn letterbox_keeps_the_whole_picture() {
        // 16:9 white: 64 × 36 in the middle, 14 rows of black above and below.
        let out = scale_to_square(&vec![255u8; 160 * 90 * 3], 160, 90, Fit::Letterbox);
        let row = |y: usize| &out[y * SIZE * 3..(y + 1) * SIZE * 3];
        assert!(row(13).iter().all(|&v| v == 0) && row(50).iter().all(|&v| v == 0));
        assert!(row(14).iter().all(|&v| v == 255) && row(49).iter().all(|&v| v == 255));
        // Tall: bars left and right.
        let out = scale_to_square(&vec![255u8; 90 * 160 * 3], 90, 160, Fit::Letterbox);
        assert!(out[13 * 3] == 0 && out[14 * 3] == 255 && out[49 * 3] == 255 && out[50 * 3] == 0);
    }

    #[test]
    fn pan_slides_across_a_wide_picture() {
        // 16:9, a left-to-right ramp (not from black, which would read as a bar):
        // the strip is 114 wide, so the pan travels 50.
        let mut img = vec![0u8; 160 * 90 * 3];
        for (i, p) in img.chunks_exact_mut(3).enumerate() {
            p.fill((64 + (i % 160) * 191 / 159) as u8);
        }
        let strip = scale_to_strip(&img, 160, 90).unwrap();
        assert_eq!(strip.w, 114);
        let square = strip.left();
        assert_eq!(square[SIZE * 3..][..SIZE * 3], strip.rgb[strip.w * 3..][..SIZE * 3]);
        assert_eq!(pan_offset(PAN_HOLD_MS, 50), 0.0);
        assert_eq!(pan_offset(PAN_HOLD_MS + 50 * PAN_MS_PER_PX, 50), 50.0);
        assert_eq!(pan_offset(2 * (PAN_HOLD_MS + 50 * PAN_MS_PER_PX), 50), 0.0);
        let mut c = Compositor::default();
        c.show(Some(Arc::from(square)), "a", 0, 0, Transition::Cut);
        c.pan(Some(Arc::new(strip)));
        let mut f = [0u16; PIXELS];
        c.render(0, &mut f);
        let left = f[0];
        c.render(PAN_HOLD_MS + 50 * PAN_MS_PER_PX, &mut f);
        assert!(f[0] > left); // the right end is brighter
        // Square and tall pictures don't pan.
        assert!(scale_to_strip(&vec![0u8; 64 * 64 * 3], 64, 64).is_none());
        assert!(scale_to_strip(&vec![0u8; 90 * 160 * 3], 90, 160).is_none());
    }

    #[test]
    fn bars_in_the_picture_are_trimmed() {
        // YouTube's 480 × 360 thumbnail: a 16:9 picture (gray) between 45-row black bars.
        let mut img = vec![0u8; 480 * 360 * 3];
        img[45 * 480 * 3..315 * 480 * 3].fill(200);
        let out = scale_to_square(&img, 480, 360, Fit::Crop);
        assert!(out.iter().all(|&v| v == 200));
        // Shown whole: the bars are the panel's 14 rows, as for a true 16:9 picture.
        let out = scale_to_square(&img, 480, 360, Fit::Letterbox);
        assert!(out[13 * SIZE * 3] == 0 && out[14 * SIZE * 3] == 200 && out[49 * SIZE * 3] == 200 && out[50 * SIZE * 3] == 0);
        // A square cover with a black border keeps it.
        let mut cover = vec![0u8; 64 * 64 * 3];
        cover[16 * 64 * 3..48 * 64 * 3].fill(200);
        assert_eq!(scale_to_square(&cover, 64, 64, Fit::Crop)[0], 0);
    }

    #[test]
    fn current_limit() {
        let white = [0xFFFFu16; PIXELS];
        let black = [0u16; PIXELS];
        assert_eq!(estimate_ma(&black, 255), IDLE_MA);
        assert!(estimate_ma(&white, 255) >= FULL_WHITE_MA - 10);
        let b = limited_brightness(&white, 255, 2400);
        assert!(b < 255 && estimate_ma(&white, b) <= 2400);
        assert_eq!(limited_brightness(&black, 200, 2400), 200);
    }

    #[test]
    fn colour_grade() {
        assert!(Grade::new(&Color::default()).is_none());
        let px = |c: Color, p: u16| {
            let mut out = [0u16; PIXELS];
            Grade::new(&c).unwrap().apply(&[p; PIXELS], &mut out);
            expand(out[0])
        };
        let gray = rgb565(64.0, 64.0, 64.0);
        let white = Color { green_pct: 50, ..Color::default() };
        assert_eq!(px(white, 0xFFFF), [255, 130, 255]); // 127.5 rounds up, then to 6 bits
        // Shadows lift or deepen dark gray; black and white stay.
        let lift = Color { shadows: 50, ..Color::default() };
        assert!(px(lift, gray)[0] > 100);
        assert!(px(Color { shadows: -50, ..lift }, gray)[0] < 40);
        assert_eq!(px(lift, 0xFFFF), [255; 3]);
        assert_eq!(px(lift, 0), [0; 3]);
        // Saturation moves colours from gray; gray stays gray.
        let flat = Color { saturation_pct: 50, ..Color::default() };
        let [r, g, _] = px(flat, rgb565(255.0, 0.0, 0.0));
        assert!(r < 255 && g > 0);
        assert_eq!(px(Color { saturation_pct: 150, ..flat }, gray), expand(gray));
    }

    #[test]
    fn calibration_image_areas() {
        let p = calibration_image();
        let at = |x: usize, y: usize| &p[(y * SIZE + x) * 3..][..3];
        assert_eq!((at(0, 0), at(63, 0)), (&[255; 3][..], &[128; 3][..]));
        assert_eq!(at(63, 20), &[80; 3]);
        assert_eq!(at(0, 36), &[255, 0, 0]);
        assert_eq!(at(63, 63), &[255; 3]);
    }

    #[test]
    fn compositor_fades() {
        let red = solid(255, 0, 0);
        let mut c = Compositor::default();
        let mut f = [0u16; PIXELS];
        c.show(Some(red), "red", 0, 400, Transition::Crossfade);
        c.render(0, &mut f);
        assert_eq!(f[0], 0); // starts black
        c.render(200, &mut f);
        let r5 = (f[0] >> 11) & 0x1F;
        assert!(r5 > 10 && r5 < 22);
        c.render(400, &mut f);
        assert_eq!(f[0], 0xF800);
        assert!(!c.render(500, &mut f)); // unchanged
        c.show(None, "", 1000, 400, Transition::Crossfade);
        c.render(1400, &mut f);
        assert_eq!(f[0], 0);
    }

    #[test]
    fn compositor_transitions() {
        let (red, blue) = (solid(255, 0, 0), solid(0, 0, 255));
        let halfway = |how| {
            let mut c = Compositor::default();
            let mut f = [0u16; PIXELS];
            c.show(Some(red.clone()), "red", 0, 0, Transition::Crossfade);
            c.render(0, &mut f);
            c.show(Some(blue.clone()), "blue", 1000, 400, how);
            c.render(1200, &mut f);
            f
        };
        const R: u16 = 0xF800;
        const B: u16 = 0x001F;
        let f = halfway(Transition::Cut);
        assert!(f[0] == B && f[PIXELS - 1] == B);
        assert_eq!(halfway(Transition::FadeBlack)[0], 0);
        let f = halfway(Transition::Crossfade);
        assert!(f[0] != R && f[0] != B && (f[0] >> 11) > 0 && (f[0] & 0x1F) > 0);
        let f = halfway(Transition::Slide); // the old picture's right half, then the new one
        assert!(f[0] == R && f[SIZE / 2 - 1] == R && f[SIZE / 2 + 1] == B && f[SIZE - 1] == B);
        let f = halfway(Transition::Wipe); // left already new, right still old
        assert!(f[0] == B && f[SIZE - 1] == R);
        let blues = halfway(Transition::Dissolve).iter().filter(|&&p| p == B).count();
        assert!(blues > PIXELS * 2 / 5 && blues < PIXELS * 3 / 5);
        for how in [Transition::Slide, Transition::Wipe, Transition::Dissolve, Transition::FadeBlack] {
            let mut c = Compositor::default();
            let mut f = [0u16; PIXELS];
            c.show(Some(red.clone()), "red", 0, 0, Transition::Crossfade);
            c.render(0, &mut f);
            c.show(Some(blue.clone()), "blue", 1000, 400, how);
            c.render(1400, &mut f);
            assert!(f[0] == B && f[PIXELS - 1] == B, "{how:?} ends on the new picture");
        }
    }

    #[test]
    fn test_pattern_corners() {
        let p = test_pattern();
        assert_eq!(&p[0..3], &[255, 0, 0]);
        assert_eq!(&p[(63 * SIZE + 63) * 3..][..3], &[255, 255, 255]);
    }
}
