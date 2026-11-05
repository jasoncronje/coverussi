//! The last few songs, for the web app: identified records and streamed plays
//! together, newest first. Small enough to keep in NVS across restarts.

use std::collections::VecDeque;

use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::album_lock::Decision;
use crate::plays::Play;
use crate::Match;

pub const KEEP: usize = 10;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Song {
    /// Unix seconds.
    pub at: f64,
    /// The analog source's name ("Turntable"), or the player's entity.
    pub source: String,
    pub title: String,
    pub artist: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub album: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub art_url: Option<String>,
    /// Identified by listening (a correction can fix its album), not streamed.
    #[serde(default)]
    pub heard: bool,
    /// The album Shazam gave, which a correction is keyed on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shazam_album: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Recent {
    songs: VecDeque<Song>,
}

impl Recent {
    /// What `to_json` saved; empty if it doesn't parse.
    pub fn from_json(s: &str) -> Self {
        serde_json::from_str(s).unwrap_or_default()
    }

    pub fn to_json(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "[]".into())
    }

    /// Newest first.
    pub fn songs(&self) -> impl Iterator<Item = &Song> {
        self.songs.iter()
    }

    /// Adds a song, unless it's the newest one again (a relisten of the same track).
    /// True when the list changed.
    pub fn add(&mut self, song: Song) -> bool {
        if let Some(top) = self.songs.front_mut() {
            if top.title == song.title && top.artist == song.artist && top.source == song.source {
                if top.album == song.album && top.art_url == song.art_url {
                    return false;
                }
                *top = Song { at: top.at, ..song }; // a better album for the same play
                return true;
            }
        }
        self.songs.push_front(song);
        self.songs.truncate(KEEP);
        true
    }

    pub fn heard(&mut self, at: f64, source: &str, m: &Match, d: &Decision) -> bool {
        self.add(Song {
            at,
            source: source.into(),
            title: m.title.clone(),
            artist: m.artist.clone(),
            album: d.album.clone(),
            art_url: d.art_url.clone(),
            heard: true,
            shazam_album: m.album.clone(),
        })
    }

    pub fn streamed(&mut self, p: &Play, album: Option<String>, art_url: Option<String>) -> bool {
        self.add(Song {
            at: p.at,
            source: p.source.clone(),
            title: p.title.clone(),
            artist: p.artist.clone(),
            album,
            art_url,
            heard: false,
            shazam_album: None,
        })
    }
}

/// The answer the engine gets for a matched clip (`engine::parse_recognition` reads it).
pub fn matched(m: &Match, d: &Decision, clip_ms: u32) -> Value {
    json!({
        "match": true,
        "title": m.title,
        "artist": m.artist,
        "album": d.album,
        "art_url": d.art_url,
        "offset_ms": m.offset_ms,
        "duration_ms": m.duration_ms,
        "ends_in_ms": m.ends_in_ms(clip_ms),
        "album_locked": d.locked,
        "reason": d.reason,
    })
}

pub fn no_match() -> Value {
    json!({"match": false, "reason": "no_match"})
}

pub fn rate_limited(retry_in_ms: u32) -> Value {
    json!({"match": false, "reason": "rate_limited", "retry_in_ms": retry_in_ms})
}

pub fn failed() -> Value {
    json!({"match": false, "reason": "error", "retry_in_ms": 30_000})
}

#[cfg(test)]
mod tests {
    use super::*;

    fn song(title: &str, album: Option<&str>) -> Song {
        Song { at: 1.0, source: "Turntable".into(), title: title.into(), artist: "Steely Dan".into(), album: album.map(String::from), ..Default::default() }
    }

    #[test]
    fn keeps_the_last_ten_newest_first() {
        let mut r = Recent::default();
        for i in 0..12 {
            assert!(r.add(song(&format!("t{i}"), None)));
        }
        let titles: Vec<_> = r.songs().map(|s| s.title.as_str()).collect();
        assert_eq!(titles.len(), KEEP);
        assert_eq!((titles[0], titles[9]), ("t11", "t2"));
    }

    #[test]
    fn the_same_track_again_is_one_entry() {
        let mut r = Recent::default();
        assert!(r.add(song("Peg", Some("Aja"))));
        assert!(!r.add(Song { at: 9.0, ..song("Peg", Some("Aja")) }));
        assert!(r.add(Song { at: 9.0, ..song("Peg", Some("Aja (Remaster)")) }));
        let s: Vec<_> = r.songs().collect();
        assert_eq!((s.len(), s[0].at, s[0].album.as_deref()), (1, 1.0, Some("Aja (Remaster)")));
        assert!(r.add(song("Deacon Blues", Some("Aja"))));
        assert!(r.add(song("Peg", Some("Aja")))); // not the newest: a new play
        assert_eq!(r.songs().count(), 3);
    }

    #[test]
    fn saves_and_loads() {
        let mut r = Recent::default();
        r.add(song("Peg", Some("Aja")));
        r.add(Song { heard: true, shazam_album: Some("Aja".into()), ..song("Josie", None) });
        let back = Recent::from_json(&r.to_json());
        assert_eq!(back, r);
        assert_eq!(Recent::from_json("not json"), Recent::default());
    }

    #[test]
    fn answers() {
        let m = Match {
            title: "Peg".into(),
            artist: "Steely Dan".into(),
            album: Some("Aja".into()),
            year: None,
            label: None,
            isrc: None,
            shazam_key: None,
            apple_id: None,
            collection_id: None,
            art_url: Some("https://a/600x600bb.jpg".into()),
            offset_ms: 30_000,
            duration_ms: Some(237_000),
            skew: 0.0,
        };
        let d = Decision { album: Some("Aja".into()), art_url: m.art_url.clone(), locked: false, reason: "new".into() };
        let a = matched(&m, &d, 12_000);
        assert_eq!((a["match"].as_bool(), a["ends_in_ms"].as_u64()), (Some(true), Some(195_000)));
        let r = crate::engine::parse_recognition(&a);
        assert!(r.matched && r.ends_in_ms == Some(195_000) && r.album == "Aja");
        assert_eq!(crate::engine::parse_recognition(&rate_limited(5000)).retry_in_ms, 5000);
        assert_eq!(crate::engine::parse_recognition(&no_match()).reason, "no_match");
    }
}
