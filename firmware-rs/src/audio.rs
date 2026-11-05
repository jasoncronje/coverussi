//! The microphones: 50 ms levels to the engine while it wants them, and clips
//! for recognition.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::sync::mpsc::{self, SyncSender};
use std::sync::Mutex;

use crate::app::{self, Msg};
use crate::{board, work};

const BLOCK: usize = board::SAMPLE_RATE / 20;

static ON: AtomicBool = AtomicBool::new(false);
static OK: AtomicBool = AtomicBool::new(false);
static RECORD_MS: AtomicU32 = AtomicU32::new(0);
/// The analog source a clip is for, to show with its recognition.
static SOURCE: Mutex<String> = Mutex::new(String::new());
/// f32 bits; u32::MAX: no change.
static GAIN: AtomicU32 = AtomicU32::new(u32::MAX);

pub fn ok() -> bool {
    OK.load(Ordering::Relaxed)
}

pub fn enable(on: bool) {
    ON.store(on, Ordering::Relaxed);
}

pub fn set_gain(db: f64) {
    GAIN.store((db as f32).to_bits(), Ordering::Relaxed);
}

/// Record a clip of `ms`; it goes to recognition, or the engine hears ClipFailed.
pub fn record(ms: u32, source: &str) {
    if !ok() {
        record_fallback(ms, source);
        return;
    }
    *SOURCE.lock().unwrap() = source.to_string();
    RECORD_MS.store(ms, Ordering::Relaxed);
}

#[cfg(not(feature = "qemu"))]
fn record_fallback(_: u32, _: &str) {
    app::post(Msg::ClipFailed);
}

/// QEMU has no microphones: the clip is the recording the build was given.
#[cfg(feature = "qemu")]
fn record_fallback(_: u32, source: &str) {
    let pcm: Vec<i16> = include_bytes!("../../build/qemu/clip.raw")
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect();
    let ms = (pcm.len() * 1000 / board::SAMPLE_RATE) as u32;
    let (tx, rx) = mpsc::sync_channel(1);
    let _ = tx.send(pcm);
    work::identify_stream(rx, ms, source.to_string());
}

fn dbfs(s: &[i16]) -> f32 {
    let sum: f64 = s.iter().map(|&x| x as f64 * x as f64).sum();
    let rms = (sum / s.len() as f64).sqrt();
    if rms > 0.0 { (20.0 * (rms / 32768.0).log10()) as f32 } else { -120.0 }
}

pub fn start(gain_db: f64) {
    if !board::mic_begin(gain_db as f32) {
        #[cfg(not(feature = "qemu"))]
        log::error!("microphones didn't start; analog sources can't be identified");
        #[cfg(feature = "qemu")]
        start_fake_levels();
        return;
    }
    OK.store(true, Ordering::Relaxed);
    crate::spawn_in_psram("mic", 6144, || {
        let mut block = vec![0i16; BLOCK];
        // The clip goes to recognition block by block, fingerprinted as it records.
        let mut clip: Option<SyncSender<Vec<i16>>> = None;
        let mut left = 0;
        loop {
            let g = GAIN.swap(u32::MAX, Ordering::Relaxed);
            if g != u32::MAX {
                board::mic_gain(f32::from_bits(g));
            }
            // Keep reading while off so the DMA never hands over stale audio.
            if !board::mic_read(&mut block) {
                std::thread::sleep(std::time::Duration::from_millis(50));
                continue;
            }
            let ms = RECORD_MS.swap(0, Ordering::Relaxed);
            if ms > 0 {
                left = ms as usize * board::SAMPLE_RATE / 1000;
                // Room for the whole clip, so the mic never waits on the fingerprint.
                let (tx, rx) = mpsc::sync_channel(left.div_ceil(BLOCK));
                clip = Some(tx);
                work::identify_stream(rx, ms, SOURCE.lock().unwrap().clone());
            }
            if let Some(tx) = &clip {
                let take = left.min(block.len());
                left -= take;
                // Recognition hung up: it matched before the clip's end.
                if tx.try_send(block[..take].to_vec()).is_err() || left == 0 {
                    clip = None;
                }
            }
            if ON.load(Ordering::Relaxed) {
                app::post(Msg::Level(dbfs(&block)));
            }
        }
    });
}

/// QEMU: music all the time, so listening to the room identifies the clip.
#[cfg(feature = "qemu")]
fn start_fake_levels() {
    std::thread::spawn(|| loop {
        std::thread::sleep(std::time::Duration::from_millis(50));
        if ON.load(Ordering::Relaxed) {
            app::post(Msg::Level(-30.0));
        }
    });
}
