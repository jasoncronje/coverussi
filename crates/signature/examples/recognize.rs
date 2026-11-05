//! Prints the body shazamio would POST for a clip, so a signature can be
//! checked against Shazam by hand:
//!
//!   afconvert -f WAVE -d LEI16@16000 -c 1 song.m4a clip.wav
//!   cargo run -p signature --example recognize -- clip.wav > body.json
//!
//! The WAV must be 16 kHz mono 16-bit, the board's format. Like shazamio, it
//! takes at most 12 s from the middle.

use std::time::{SystemTime, UNIX_EPOCH};

fn main() {
    let path = std::env::args().nth(1).expect("usage: recognize <clip.wav>");
    let bytes = std::fs::read(&path).expect("read clip");
    let samples = pcm(&bytes);
    let max = 12 * signature::SAMPLE_RATE_HZ as usize;
    let clip = if samples.len() > max {
        let mid = samples.len() / 2;
        &samples[mid - max / 2..mid + max / 2]
    } else {
        &samples[..]
    };
    let sig = signature::signature(clip);
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis();
    println!(
        r#"{{"timezone":"Europe/Paris","signature":{{"uri":"{}","samplems":{}}},"timestamp":{},"context":{{}},"geolocation":{{}}}}"#,
        sig.uri, sig.sample_ms, now
    );
}

/// The samples of a 16 kHz mono s16 WAV's `data` chunk.
fn pcm(wav: &[u8]) -> Vec<i16> {
    let u32_at = |i: usize| u32::from_le_bytes(wav[i..i + 4].try_into().unwrap());
    let mut i = 12;
    while i + 8 <= wav.len() {
        let (id, size) = (&wav[i..i + 4], u32_at(i + 4) as usize);
        if id == b"fmt " {
            let channels = u16::from_le_bytes([wav[i + 10], wav[i + 11]]);
            let rate = u32_at(i + 12);
            assert!(channels == 1 && rate == 16000, "need 16 kHz mono, got {rate} Hz × {channels}");
        }
        if id == b"data" {
            let end = (i + 8 + size).min(wav.len());
            return wav[i + 8..end]
                .chunks_exact(2)
                .map(|b| i16::from_le_bytes([b[0], b[1]]))
                .collect();
        }
        i += 8 + size + (size & 1);
    }
    panic!("no data chunk");
}
