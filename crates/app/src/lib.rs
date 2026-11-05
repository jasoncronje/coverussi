//! What the board decides, kept free of networking and hardware so it runs and
//! is tested on the host: the Shazam and iTunes answers, the album a listening
//! session shows, which streamed songs count as plays, and the settings.

pub mod album_lock;
pub mod art;
pub mod auth;
pub mod engine;
pub mod ha;
pub mod idle;
pub mod itunes;
pub mod keys;
pub mod model;
pub mod mqtt;
pub mod plays;
pub mod recent;
pub mod settings;
pub mod shazam;
pub mod throttle;
pub mod tz;
pub mod ws;
mod zones;

/// One identified clip.
#[derive(Clone, Debug, PartialEq)]
pub struct Match {
    pub title: String,
    pub artist: String,
    pub album: Option<String>,
    pub year: Option<String>,
    pub label: Option<String>,
    pub isrc: Option<String>,
    pub shazam_key: Option<String>,
    pub apple_id: Option<String>,
    /// The iTunes album, for its track list.
    pub collection_id: Option<String>,
    pub art_url: Option<String>,
    /// Where in the track the clip starts.
    pub offset_ms: u32,
    /// The track's length, from iTunes.
    pub duration_ms: Option<u32>,
    /// Shazam's largest time or frequency skew; wrong answers skew more.
    pub skew: f64,
}

impl Match {
    /// Time from the end of the clip to the end of the track.
    pub fn ends_in_ms(&self, clip_ms: u32) -> Option<u32> {
        self.duration_ms
            .map(|d| d.saturating_sub(self.offset_ms).saturating_sub(clip_ms))
    }
}
