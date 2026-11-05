//! Which streamed songs join the history: a digital source's song counts once
//! it has been the one showing for `PLAY_AFTER_S`. A skip past it sooner isn't
//! a play, and a pause and resume of the same song isn't a second one.

pub const PLAY_AFTER_S: f64 = 10.0;

/// What's showing, from the display's state.
pub struct Showing<'a> {
    pub digital: bool,
    pub source: &'a str,
    pub artist: &'a str,
    pub title: &'a str,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Play {
    /// When it started showing.
    pub at: f64,
    pub source: String,
    pub artist: String,
    pub title: String,
}

type Key = (String, String, String);

#[derive(Default)]
pub struct PlayFollower {
    candidate: Option<(Key, f64)>,
    last: Option<Key>,
}

impl PlayFollower {
    /// Feed every state change; returns the play to log, once.
    pub fn update(&mut self, now: f64, s: &Showing) -> Option<Play> {
        if !s.digital || s.title.is_empty() {
            self.candidate = None;
            return None;
        }
        let key = (s.source.to_string(), s.artist.to_string(), s.title.to_string());
        if self.last.as_ref() == Some(&key) {
            return None;
        }
        let started = match &self.candidate {
            Some((k, at)) if *k == key => *at,
            _ => {
                self.candidate = Some((key, now));
                return None;
            }
        };
        if now - started < PLAY_AFTER_S {
            return None;
        }
        self.candidate = None;
        self.last = Some(key.clone());
        let (source, artist, title) = key;
        Some(Play { at: started, source, artist, title })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn digital(title: &str) -> Showing<'_> {
        Showing { digital: true, source: "media_player.apple_tv", artist: "ROM COM", title }
    }

    #[test]
    fn streamed_songs_become_plays() {
        let mut f = PlayFollower::default();
        assert_eq!(f.update(1000.0, &digital("Come Thru")), None);
        assert_eq!(f.update(1008.0, &digital("Come Thru")), None); // 8 s: not yet
        let play = f.update(1010.0, &digital("Come Thru")).unwrap();
        assert_eq!((play.at, play.title.as_str(), play.source.as_str()), (1000.0, "Come Thru", "media_player.apple_tv"));

        // Paused and resumed, or the same state again: still one play.
        assert_eq!(f.update(1100.0, &digital("Come Thru")), None);
        assert_eq!(f.update(1200.0, &digital("Come Thru")), None);

        // A song skipped after 4 s isn't a play; the next one is.
        f.update(1300.0, &digital("Skipped"));
        f.update(1304.0, &digital("Skipped"));
        f.update(1305.0, &digital("Next"));
        assert_eq!(f.update(1316.0, &digital("Next")).map(|p| p.title), Some("Next".into()));

        // Listening to a record never makes a streamed play.
        let analog = Showing { digital: false, source: "", artist: "", title: "Dreams" };
        assert_eq!(f.update(1400.0, &analog), None);
        assert_eq!(f.update(1420.0, &analog), None);
    }
}
