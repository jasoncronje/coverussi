//! The Shazam signature of a 16 kHz mono clip, the same one shazamio sends.
//!
//! `algorithm.rs`, `format.rs` and `hanning.rs` are shazamio-core's
//! fingerprinting (MIT, `LICENSE-shazamio-core`), taken at
//! shazamio/shazamio-core@f6076da with only the file decoding and resampling
//! left out: the board records 16 kHz mono already.

mod algorithm;
mod format;
mod hanning;

use algorithm::SignatureGenerator;

pub const SAMPLE_RATE_HZ: u32 = 16000;

pub struct Signature {
    /// `data:audio/vnd.shazam.sig;base64,...`
    pub uri: String,
    /// The clip's length in milliseconds, which Shazam calls `samplems`.
    pub sample_ms: u32,
}

/// A clip fingerprinted as it records: feed it blocks of any size, and take
/// the signature of what it has heard so far whenever it's wanted.
pub struct Fingerprint {
    generator: SignatureGenerator,
    /// Samples short of a whole 128-sample chunk.
    pending: Vec<i16>,
    samples: usize,
}

impl Default for Fingerprint {
    fn default() -> Self {
        Self { generator: SignatureGenerator::new(), pending: Vec::with_capacity(128), samples: 0 }
    }
}

impl Fingerprint {
    pub fn feed(&mut self, mut samples: &[i16]) {
        self.samples += samples.len();
        if !self.pending.is_empty() {
            let take = (128 - self.pending.len()).min(samples.len());
            self.pending.extend_from_slice(&samples[..take]);
            samples = &samples[take..];
            if self.pending.len() < 128 {
                return;
            }
            let chunk = std::mem::take(&mut self.pending);
            self.generator.feed(&chunk);
            self.pending = chunk;
            self.pending.clear();
        }
        let mut chunks = samples.chunks_exact(128);
        for chunk in &mut chunks {
            self.generator.feed(chunk);
        }
        self.pending.extend_from_slice(chunks.remainder());
    }

    /// How much has been fed, in milliseconds.
    pub fn ms(&self) -> u32 {
        (self.samples as u64 * 1000 / SAMPLE_RATE_HZ as u64) as u32
    }

    pub fn signature(&mut self) -> Signature {
        let sample_ms = self.ms();
        let decoded = self.generator.signature(self.samples as u32);
        // Writes into a `Vec`, which can't fail.
        Signature { uri: decoded.encode_to_uri().expect("encode signature"), sample_ms }
    }
}

pub fn signature(samples: &[i16]) -> Signature {
    let mut f = Fingerprint::default();
    f.feed(samples);
    f.signature()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tone(seconds: u32) -> Vec<i16> {
        (0..seconds * SAMPLE_RATE_HZ)
            .map(|n| {
                let t = n as f32 / SAMPLE_RATE_HZ as f32;
                // A chord that changes every half second, so there are peaks to find.
                let step = (t * 2.0) as u32 % 4;
                let f = [440.0, 554.4, 659.3, 880.0][step as usize];
                ((t * f * std::f32::consts::TAU).sin() * 8000.0) as i16
            })
            .collect()
    }

    #[test]
    fn a_clip_gives_a_data_uri_and_its_length() {
        let sig = signature(&tone(10));
        assert!(sig.uri.starts_with("data:audio/vnd.shazam.sig;base64,"));
        assert_eq!(sig.sample_ms, 10_000);
        // Peaks were found: an empty signature is just the 48-byte header and band tags.
        assert!(sig.uri.len() > 200, "{}", sig.uri.len());
    }

    #[test]
    fn fed_in_blocks_matches_fed_at_once() {
        let clip = tone(6);
        let mut f = Fingerprint::default();
        for block in clip.chunks(800 + 7) {
            f.feed(block);
        }
        let whole = signature(&clip);
        let streamed = f.signature();
        assert_eq!((streamed.uri, streamed.sample_ms), (whole.uri, whole.sample_ms));
        // A signature part way through, then more audio.
        let mut f = Fingerprint::default();
        f.feed(&clip[..3 * SAMPLE_RATE_HZ as usize]);
        assert_eq!(f.signature().sample_ms, 3000);
        f.feed(&clip[3 * SAMPLE_RATE_HZ as usize..]);
        assert_eq!(f.signature().uri, signature(&clip).uri);
    }

    #[test]
    fn silence_gives_no_peaks_but_still_encodes() {
        let sig = signature(&vec![0; 3 * SAMPLE_RATE_HZ as usize]);
        assert_eq!(sig.sample_ms, 3_000);
    }
}
