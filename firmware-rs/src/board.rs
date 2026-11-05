//! The panel, the microphones and JPEG decoding: components/board's C API.
//! The QEMU build has none of the hardware, so there the panel draws nothing
//! and the mics never start.

pub const SAMPLE_RATE: usize = 16000;

#[cfg(not(feature = "qemu"))]
mod ffi {
    extern "C" {
        pub fn board_panel_begin(brightness: u8) -> bool;
        pub fn board_panel_draw_rgb565(pixels: *const u16);
        pub fn board_panel_brightness(brightness: u8);
        pub fn board_mic_begin(gain_db: f32) -> bool;
        pub fn board_mic_gain(gain_db: f32);
        pub fn board_mic_read(out: *mut i16, samples: usize) -> bool;
    }
}

extern "C" {
    fn board_jpeg_info(jpeg: *const u8, len: usize, min_side: i32, scale: *mut i32, buffer: *mut usize) -> bool;
    fn board_jpeg_decode(
        jpeg: *const u8,
        len: usize,
        scale: i32,
        out: *mut u8,
        out_len: usize,
        w: *mut i32,
        h: *mut i32,
    ) -> bool;
}

#[cfg(not(feature = "qemu"))]
pub fn panel_begin() -> bool {
    unsafe { ffi::board_panel_begin(0) }
}
#[cfg(not(feature = "qemu"))]
pub fn panel_draw(f: &coverussi_app::art::Frame) {
    unsafe { ffi::board_panel_draw_rgb565(f.as_ptr()) }
}
#[cfg(not(feature = "qemu"))]
pub fn panel_brightness(b: u8) {
    unsafe { ffi::board_panel_brightness(b) }
}
#[cfg(not(feature = "qemu"))]
pub fn mic_begin(gain_db: f32) -> bool {
    unsafe { ffi::board_mic_begin(gain_db) }
}
#[cfg(not(feature = "qemu"))]
pub fn mic_gain(gain_db: f32) {
    unsafe { ffi::board_mic_gain(gain_db) }
}
/// Fill `out` with mono samples; blocks until it has them.
#[cfg(not(feature = "qemu"))]
pub fn mic_read(out: &mut [i16]) -> bool {
    unsafe { ffi::board_mic_read(out.as_mut_ptr(), out.len()) }
}

#[cfg(feature = "qemu")]
pub fn panel_begin() -> bool {
    true
}
#[cfg(feature = "qemu")]
pub fn panel_draw(_: &coverussi_app::art::Frame) {}
#[cfg(feature = "qemu")]
pub fn panel_brightness(_: u8) {}
#[cfg(feature = "qemu")]
pub fn mic_begin(_: f32) -> bool {
    false
}
#[cfg(feature = "qemu")]
pub fn mic_gain(_: f32) {}
#[cfg(feature = "qemu")]
pub fn mic_read(_: &mut [i16]) -> bool {
    false
}

/// A baseline JPEG as RGB888, scaled down by the decoder while its short side
/// stays at least `min_side`: (pixels, width, height).
pub fn decode_jpeg(jpeg: &[u8], min_side: i32) -> Option<(Vec<u8>, usize, usize)> {
    let (mut scale, mut size) = (0, 0);
    if !unsafe { board_jpeg_info(jpeg.as_ptr(), jpeg.len(), min_side, &mut scale, &mut size) } {
        return None;
    }
    let mut out = vec![0u8; size];
    let (mut w, mut h) = (0, 0);
    if !unsafe { board_jpeg_decode(jpeg.as_ptr(), jpeg.len(), scale, out.as_mut_ptr(), size, &mut w, &mut h) } {
        return None;
    }
    let (w, h) = (w as usize, h as usize);
    out.truncate(w * h * 3);
    Some((out, w, h))
}
