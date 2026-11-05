//! Which album a listening session is showing.
//!
//! Song recognition names *a* release of the song, often a compilation, a single
//! or a reissue ("BD Music Presents Kind of Blue" for Kind of Blue). On a record
//! the whole side is one album, so the first match sets the session's album and
//! later matches only replace it with good reason.

use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use crate::keys::{album_key, artist_key, title_key};
use crate::Match;

/// From the portal: this artist's album (as Shazam names it) is really that album and art.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Correction {
    pub album: String,
    pub art_url: Option<String>,
}

#[derive(Debug)]
struct Session {
    artist: String,
    album: Option<String>,
    art_url: Option<String>,
    key: String,
    /// Title keys on the album, when iTunes knows them.
    tracks: HashSet<String>,
    pinned: bool,
    last_match: f64,
    /// Another album's key, seen in a row.
    challenger: Option<String>,
    challenger_count: u32,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Decision {
    pub album: Option<String>,
    pub art_url: Option<String>,
    /// The art is the session's album, not this match's own.
    pub locked: bool,
    pub reason: String,
}

pub struct AlbumLock {
    pub replace_after: u32,
    pub timeout_s: f64,
    /// Keyed by `album_key` of the album as Shazam names it.
    pub corrections: HashMap<String, Correction>,
    session: Option<Session>,
}

impl Default for AlbumLock {
    fn default() -> Self {
        Self::new(2, 900.0)
    }
}

impl AlbumLock {
    pub fn new(replace_after: u32, timeout_s: f64) -> Self {
        Self { replace_after, timeout_s, corrections: HashMap::new(), session: None }
    }

    pub fn correct(&self, m: &Match) -> Match {
        let mut m = m.clone();
        if let Some(fix) = self.corrections.get(&album_key(&m.artist, m.album.as_deref())) {
            m.album = Some(fix.album.clone());
            if fix.art_url.is_some() {
                m.art_url = fix.art_url.clone();
            }
        }
        m
    }

    pub fn end(&mut self) {
        self.session = None;
    }

    /// Keep the session's album whatever plays next. False with no session.
    pub fn pin(&mut self) -> bool {
        self.session.as_mut().map(|s| s.pinned = true).is_some()
    }

    fn start(&mut self, m: &Match, tracks: &[String], now: f64, reason: &str) -> Decision {
        self.session = Some(Session {
            artist: m.artist.clone(),
            album: m.album.clone(),
            art_url: m.art_url.clone(),
            key: album_key(&m.artist, m.album.as_deref()),
            tracks: tracks.iter().map(|t| title_key(t)).collect(),
            pinned: false,
            last_match: now,
            challenger: None,
            challenger_count: 0,
        });
        Decision { album: m.album.clone(), art_url: m.art_url.clone(), locked: false, reason: reason.into() }
    }

    /// Decide what to show for a new match. `tracks` is the match's own album's track list.
    pub fn apply(&mut self, m: &Match, now: f64, tracks: &[String]) -> Decision {
        let m = self.correct(m);
        let replace_after = self.replace_after;
        let Some(s) = self.session.as_mut() else {
            return self.start(&m, tracks, now, "new session");
        };
        if now - s.last_match > self.timeout_s && !s.pinned {
            return self.start(&m, tracks, now, "session timed out");
        }
        s.last_match = now;
        let key = album_key(&m.artist, m.album.as_deref());
        let kept = |s: &Session, locked: bool, reason: &str| Decision {
            album: s.album.clone(),
            art_url: s.art_url.clone(),
            locked,
            reason: reason.into(),
        };

        if key == s.key {
            (s.challenger, s.challenger_count) = (None, 0);
            return kept(s, false, "same album");
        }
        if s.pinned {
            return kept(s, true, "pinned");
        }
        // A match that named no album (a medley, an odd compilation) holds nothing.
        if s.album.is_none() && m.album.is_some() {
            return self.start(&m, tracks, now, "the session had no album");
        }
        let same_artist = artist_key(&m.artist) == artist_key(&s.artist);
        if same_artist && s.tracks.contains(&title_key(&m.title)) {
            (s.challenger, s.challenger_count) = (None, 0);
            return kept(s, true, "song is on the session's album");
        }
        if !same_artist {
            return self.start(&m, tracks, now, "different artist");
        }

        // Same artist, a song the session's album doesn't have: another record.
        if !s.tracks.is_empty() {
            return self.start(&m, tracks, now, "another album by the same artist");
        }
        // Same artist, another album, and no track list to tell: a new record of
        // theirs, or just another release.
        if s.challenger.as_deref() == Some(key.as_str()) {
            s.challenger_count += 1;
        } else {
            (s.challenger, s.challenger_count) = (Some(key), 1);
        }
        if s.challenger_count >= replace_after {
            let reason = format!("{replace_after} matches in a row on another album");
            return self.start(&m, tracks, now, &reason);
        }
        kept(s, true, "kept: another release by the same artist")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(title: &str, artist: &str, album: &str) -> Match {
        Match {
            title: title.into(),
            artist: artist.into(),
            album: Some(album.into()),
            year: None,
            label: None,
            isrc: None,
            shazam_key: None,
            apple_id: None,
            collection_id: None,
            art_url: Some(format!("art:{album}")),
            offset_ms: 0,
            duration_ms: None,
            skew: 0.0,
        }
    }

    fn rumours() -> Vec<String> {
        ["Second Hand News", "Dreams", "Never Going Back Again", "Don't Stop", "Go Your Own Way"]
            .map(String::from)
            .to_vec()
    }

    fn shown(d: &Decision) -> (&str, bool, &str) {
        (d.album.as_deref().unwrap(), d.locked, d.reason.as_str())
    }

    #[test]
    fn first_match_starts_session() {
        let mut lock = AlbumLock::default();
        let d = lock.apply(&m("Dreams", "Fleetwood Mac", "Rumours"), 0.0, &rumours());
        assert_eq!(shown(&d), ("Rumours", false, "new session"));
    }

    #[test]
    fn compilation_match_keeps_lp_art() {
        let mut lock = AlbumLock::default();
        lock.apply(&m("Dreams", "Fleetwood Mac", "Rumours"), 0.0, &rumours());
        let d = lock.apply(&m("Go Your Own Way", "Fleetwood Mac", "Greatest Hits"), 240.0, &[]);
        assert_eq!(d.art_url.as_deref(), Some("art:Rumours"));
        assert_eq!(shown(&d), ("Rumours", true, "song is on the session's album"));
    }

    #[test]
    fn same_artist_song_not_on_the_album_switches_at_once() {
        let mut lock = AlbumLock::new(2, 900.0);
        lock.apply(&m("Dreams", "Fleetwood Mac", "Rumours"), 0.0, &rumours());
        let d = lock.apply(&m("Tusk", "Fleetwood Mac", "Tusk"), 240.0, &[]);
        assert_eq!(shown(&d), ("Tusk", false, "another album by the same artist"));
    }

    #[test]
    fn same_artist_new_album_needs_two_in_a_row_without_a_track_list() {
        let mut lock = AlbumLock::new(2, 900.0);
        lock.apply(&m("Dreams", "Fleetwood Mac", "Rumours"), 0.0, &[]);
        let d = lock.apply(&m("Tusk", "Fleetwood Mac", "Tusk"), 240.0, &[]);
        assert_eq!((d.album.as_deref(), d.locked), (Some("Rumours"), true));
        let d = lock.apply(&m("Sara", "Fleetwood Mac", "Tusk"), 480.0, &[]);
        assert_eq!((d.album.as_deref(), d.locked), (Some("Tusk"), false));
    }

    #[test]
    fn a_session_without_an_album_gives_way_at_once() {
        let mut lock = AlbumLock::new(2, 900.0);
        let medley = Match { album: None, art_url: Some("art:compilation".into()), ..m("School Days / Bye Bye Love", "Chuck Berry & The Everly Brothers", "") };
        lock.apply(&medley, 0.0, &[]);
        let d = lock.apply(&m("Thirty Days", "Chuck Berry", "The Great Twenty-Eight"), 240.0, &[]);
        assert_eq!(shown(&d), ("The Great Twenty-Eight", false, "the session had no album"));
    }

    #[test]
    fn a_same_album_match_resets_the_challenger() {
        let mut lock = AlbumLock::new(2, 900.0);
        lock.apply(&m("Dreams", "Fleetwood Mac", "Rumours"), 0.0, &[]);
        lock.apply(&m("Tusk", "Fleetwood Mac", "Tusk"), 240.0, &[]);
        lock.apply(&m("Songbird", "Fleetwood Mac", "Rumours"), 480.0, &[]);
        let d = lock.apply(&m("Sara", "Fleetwood Mac", "Tusk"), 720.0, &[]);
        assert_eq!(d.album.as_deref(), Some("Rumours"));
    }

    #[test]
    fn different_artist_switches_at_once() {
        let mut lock = AlbumLock::default();
        lock.apply(&m("Dreams", "Fleetwood Mac", "Rumours"), 0.0, &rumours());
        let d = lock.apply(&m("Peg", "Steely Dan", "Aja"), 240.0, &[]);
        assert_eq!(shown(&d), ("Aja", false, "different artist"));
    }

    #[test]
    fn pinned_album_never_switches() {
        let mut lock = AlbumLock::default();
        lock.apply(&m("So What", "Miles Davis", "Kind of Blue"), 0.0, &[]);
        assert!(lock.pin());
        let d = lock.apply(&m("Peg", "Steely Dan", "Aja"), 240.0, &[]);
        assert_eq!((d.album.as_deref(), d.reason.as_str()), (Some("Kind of Blue"), "pinned"));
        let d = lock.apply(&m("Peg", "Steely Dan", "Aja"), 10_000.0, &[]);
        assert_eq!(d.album.as_deref(), Some("Kind of Blue"));
    }

    #[test]
    fn session_times_out() {
        let mut lock = AlbumLock::new(2, 900.0);
        lock.apply(&m("Dreams", "Fleetwood Mac", "Rumours"), 0.0, &rumours());
        let d = lock.apply(&m("Tusk", "Fleetwood Mac", "Tusk"), 2000.0, &[]);
        assert_eq!((d.album.as_deref(), d.reason.as_str()), (Some("Tusk"), "session timed out"));
    }

    #[test]
    fn correction_applies_before_everything() {
        let mut lock = AlbumLock::default();
        lock.corrections.insert(
            album_key("Miles Davis", Some("BD Music Presents Kind of Blue")),
            Correction { album: "Kind of Blue".into(), art_url: Some("art:kob".into()) },
        );
        let d = lock.apply(&m("So What", "Miles Davis", "BD Music Presents Kind of Blue"), 0.0, &[]);
        assert_eq!((d.album.as_deref(), d.art_url.as_deref()), (Some("Kind of Blue"), Some("art:kob")));
    }

    #[test]
    fn pin_without_a_session() {
        assert!(!AlbumLock::default().pin());
    }
}
