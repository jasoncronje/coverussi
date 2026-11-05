//! iTunes lookups: the track's length for a match, an album's track list, and
//! the album and cover of a streamed song. The board fetches the URLs; this
//! reads the answers.

use serde_json::Value;

use crate::keys::{artist_key, norm, title_key};
use crate::shazam::{resize_art, ART_SIZE};
use crate::Match;

const LOOKUP: &str = "https://itunes.apple.com/lookup";
const SEARCH: &str = "https://itunes.apple.com/search";

// Each URL asks one country's store (a two-letter code; empty: Apple's default, the US).

/// The song with this Apple Music ID.
pub fn lookup_url(apple_id: &str, country: &str) -> String {
    format!("{LOOKUP}?id={}&entity=song{}", encode(apple_id), store(country))
}

/// Songs matching artist and title.
pub fn search_url(artist: &str, title: &str, country: &str) -> String {
    format!("{SEARCH}?term={}&entity=song&limit=10{}", encode(&format!("{artist} {title}")), store(country))
}

/// More songs matching artist and title, to find the one on a studio album.
pub fn versions_url(artist: &str, title: &str, country: &str) -> String {
    format!("{SEARCH}?term={}&entity=song&limit=50{}", encode(&format!("{artist} {title}")), store(country))
}

/// The songs on an album.
pub fn album_url(collection_id: &str, country: &str) -> String {
    format!("{LOOKUP}?id={}&entity=song&limit=200{}", encode(collection_id), store(country))
}

fn store(country: &str) -> String {
    if country.is_empty() { String::new() } else { format!("&country={}", encode(&country.to_ascii_lowercase())) }
}

fn results(answer: &Value) -> impl Iterator<Item = &Value> {
    answer.get("results").and_then(Value::as_array).into_iter().flatten()
}

fn str_of<'a>(v: &'a Value, k: &str) -> Option<&'a str> {
    v.get(k).and_then(Value::as_str)
}

fn is_track(v: &Value) -> bool {
    str_of(v, "wrapperType") == Some("track")
}

/// The song in a lookup answer.
pub fn looked_up(answer: &Value) -> Option<&Value> {
    results(answer).find(|x| is_track(x))
}

/// The song in a search answer with exactly this title and artist (ignoring case and punctuation).
pub fn found<'a>(answer: &'a Value, artist: &str, title: &str) -> Option<&'a Value> {
    results(answer).find(|s| {
        norm(str_of(s, "trackName").unwrap_or("")) == norm(title)
            && norm(str_of(s, "artistName").unwrap_or("")) == norm(artist)
    })
}

/// Fill the length (and the album and art when Shazam had none) from an iTunes song.
pub fn enrich(m: &Match, song: Option<&Value>) -> Match {
    let mut m = m.clone();
    let Some(song) = song else { return m };
    if let Some(ms) = song.get("trackTimeMillis").and_then(Value::as_u64).filter(|&ms| ms > 0) {
        m.duration_ms = Some(ms as u32);
    }
    if m.album.is_none() {
        m.album = str_of(song, "collectionName").map(String::from);
    }
    if let Some(id) = collection_id(song) {
        m.collection_id = Some(id);
    }
    if m.art_url.is_none() {
        m.art_url = resize_art(str_of(song, "artworkUrl100"), ART_SIZE);
    }
    m
}

fn collection_id(song: &Value) -> Option<String> {
    let id = song.get("collectionId").filter(|id| !id.is_null())?;
    Some(id.as_str().map_or_else(|| id.to_string(), String::from))
}

/// Words in a release's name that make it a compilation or a live record.
const NOT_ALBUM: [&str; 12] = [
    "greatest hits", "best of", "very best", "the essential", "collection", "anthology", "hits",
    "number ones", "live at", "live in", "live from", "unplugged",
];

/// The song's release is a studio album: not a single or EP, a compilation or a live record.
pub fn on_album(song: &Value) -> bool {
    let name = str_of(song, "collectionName").unwrap_or("").to_lowercase();
    let words: Vec<&str> = name.split(|c: char| !c.is_alphanumeric()).filter(|w| !w.is_empty()).collect();
    let phrase = format!(" {} ", words.join(" "));
    let various = str_of(song, "collectionArtistName").is_some_and(|a| norm(a) == "variousartists");
    let tracks = song.get("trackCount").and_then(Value::as_u64).unwrap_or(0);
    !words.is_empty()
        && !name.ends_with(" - single")
        && !name.ends_with(" - ep")
        && name != "live"
        && !various
        && tracks >= 5
        && !NOT_ALBUM.iter().any(|w| phrase.contains(&format!(" {w} ")))
}

/// The song on its earliest studio album, from a search answer. Remasters and
/// "- 2011 Remaster" titles count as the same song; "(Live)" doesn't.
pub fn album_version<'a>(answer: &'a Value, artist: &str, title: &str) -> Option<&'a Value> {
    let (artist, title) = (artist_key(artist), title_key(title));
    results(answer)
        .filter(|s| is_track(s) && on_album(s))
        .filter(|s| title_key(str_of(s, "trackName").unwrap_or("")) == title)
        .filter(|s| artist_key(str_of(s, "artistName").unwrap_or("")) == artist)
        .min_by_key(|s| str_of(s, "releaseDate").unwrap_or("9999"))
}

/// The match shown with this store song's album and art: the album version, or
/// the song in the store when Shazam's release isn't sold there.
pub fn use_album_version(m: &Match, song: &Value) -> Match {
    let mut m = m.clone();
    if let Some(album) = str_of(song, "collectionName") {
        m.album = Some(album.to_string());
    }
    if let Some(art) = resize_art(str_of(song, "artworkUrl100"), ART_SIZE) {
        m.art_url = Some(art);
    }
    m.collection_id = collection_id(song).or(m.collection_id);
    m
}

/// The titles in an album lookup.
pub fn album_tracks(answer: &Value) -> Vec<String> {
    results(answer).filter(|x| is_track(x)).filter_map(|x| str_of(x, "trackName")).map(String::from).collect()
}

/// A streamed song's album and cover, from a search answer.
pub fn song_art(answer: &Value, artist: &str, title: &str) -> Option<(Option<String>, Option<String>)> {
    let song = found(answer, artist, title)?;
    Some((str_of(song, "collectionName").map(String::from), resize_art(str_of(song, "artworkUrl100"), ART_SIZE)))
}

/// Percent-encodes a query value, spaces as `+` like a form.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => out.push(b as char),
            b' ' => out.push('+'),
            _ => out.push_str(&format!("%{b:02X}")),
        }
    }
    out
}

/// Apple's artwork at the size the panel needs (128 px, which decodes at half
/// scale to 64): a fraction of the 600 px file. Other URLs stay as they are.
pub fn panel_art_url(url: &str) -> String {
    let Some((base, file)) = url.rsplit_once('/') else { return url.to_string() };
    let sized = file.split_once('x').is_some_and(|(w, rest)| {
        !w.is_empty() && w.bytes().all(|b| b.is_ascii_digit()) && rest.starts_with(|c: char| c.is_ascii_digit())
    });
    if url.contains(".mzstatic.com/image/thumb/") && sized && file.ends_with(".jpg") {
        format!("{base}/128x128bb.jpg")
    } else {
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_art_is_small() {
        let big = "https://is1-ssl.mzstatic.com/image/thumb/Music221/v4/bb/45/x.rgb.jpg/600x600cc.jpg";
        assert_eq!(panel_art_url(big), "https://is1-ssl.mzstatic.com/image/thumb/Music221/v4/bb/45/x.rgb.jpg/128x128bb.jpg");
        let proxy = "/api/media_player_proxy/media_player.tv?token=1";
        assert_eq!(panel_art_url(proxy), proxy);
        assert_eq!(panel_art_url("https://example.com/600x600cc.jpg"), "https://example.com/600x600cc.jpg");
    }
    use serde_json::json;

    fn peg() -> Match {
        let raw = serde_json::from_str(include_str!("../tests/fixtures/shazam_peg.json")).unwrap();
        crate::shazam::parse(&raw).unwrap()
    }

    #[test]
    fn enrich_and_ends_in() {
        let m = peg();
        let song = json!({"trackTimeMillis": 237066, "collectionId": 1706428460u64, "collectionName": "Aja"});
        let e = enrich(&m, Some(&song));
        assert_eq!(e.duration_ms, Some(237066));
        assert_eq!(e.collection_id.as_deref(), Some("1706428460"));
        assert_eq!(e.ends_in_ms(10_000), Some(237066 - 44632 - 10_000));
        assert_eq!(enrich(&m, None), m);
    }

    #[test]
    fn search_picks_the_exact_song() {
        let answer = json!({"results": [
            {"wrapperType": "track", "trackName": "Peg (Live)", "artistName": "Steely Dan"},
            {"wrapperType": "track", "trackName": "PEG", "artistName": "Steely Dan", "collectionName": "Aja",
             "artworkUrl100": "https://a/1/100x100bb.jpg"},
        ]});
        let (album, art) = song_art(&answer, "Steely Dan", "Peg").unwrap();
        assert_eq!(album.as_deref(), Some("Aja"));
        assert_eq!(art.as_deref(), Some("https://a/1/600x600bb.jpg"));
        assert!(found(&answer, "Steely Dan", "Josie").is_none());
    }

    fn release(album: &str, tracks: u64, date: &str) -> Value {
        json!({"wrapperType": "track", "trackName": "Dreams", "artistName": "Fleetwood Mac", "collectionName": album,
               "trackCount": tracks, "releaseDate": date, "collectionId": 1, "artworkUrl100": format!("https://a/{album}/100x100bb.jpg")})
    }

    #[test]
    fn what_counts_as_an_album() {
        assert!(on_album(&release("Rumours", 11, "1977")));
        assert!(on_album(&release("Rumours (Super Deluxe)", 40, "2013")));
        assert!(on_album(&release("Live Through This", 12, "1994")));
        assert!(!on_album(&release("Dreams - Single", 1, "1977")));
        assert!(!on_album(&release("Dreams - EP", 4, "1977")));
        assert!(!on_album(&release("Greatest Hits", 16, "1988")));
        assert!(!on_album(&release("The Very Best of Fleetwood Mac", 36, "2002")));
        assert!(!on_album(&release("Live in Boston", 22, "2004")));
        assert!(!on_album(&release("Rumours", 3, "1977")));
        let mut various = release("Now That's What I Call the 70s", 40, "2010");
        various["collectionArtistName"] = json!("Various Artists");
        assert!(!on_album(&various));
    }

    #[test]
    fn album_version_is_the_earliest_studio_album() {
        let mut live = release("Rumours", 11, "1980");
        live["trackName"] = json!("Dreams (Live)");
        let answer = json!({"results": [
            release("Greatest Hits", 16, "1988"),
            release("Rumours (Super Deluxe)", 40, "2013"),
            live,
            release("Rumours", 11, "1977-02-04T08:00:00Z"),
            release("Dreams - Single", 1, "1977"),
        ]});
        let v = album_version(&answer, "Fleetwood Mac", "Dreams - 2004 Remaster").unwrap();
        assert_eq!(v["collectionName"], "Rumours");
        assert!(album_version(&answer, "Fleetwood Mac", "Sara").is_none());
        assert!(album_version(&answer, "Stevie Nicks", "Dreams").is_none());

        let m = use_album_version(&peg(), v);
        assert_eq!(m.album.as_deref(), Some("Rumours"));
        assert_eq!(m.art_url.as_deref(), Some("https://a/Rumours/600x600bb.jpg"));
        assert_eq!(m.collection_id.as_deref(), Some("1"));
        assert_eq!(m.duration_ms, peg().duration_ms);
    }

    #[test]
    fn album_lookup_lists_its_tracks() {
        let answer = json!({"results": [
            {"wrapperType": "collection", "collectionName": "Aja"},
            {"wrapperType": "track", "trackName": "Black Cow"},
            {"wrapperType": "track", "trackName": "Aja"},
        ]});
        assert_eq!(album_tracks(&answer), ["Black Cow", "Aja"]);
        assert_eq!(looked_up(&answer).and_then(|s| s["trackName"].as_str()), Some("Black Cow"));
    }

    #[test]
    fn urls_are_encoded() {
        assert_eq!(
            search_url("Simon & Garfunkel", "Cecilia", ""),
            "https://itunes.apple.com/search?term=Simon+%26+Garfunkel+Cecilia&entity=song&limit=10"
        );
        assert_eq!(lookup_url("1706428468", ""), "https://itunes.apple.com/lookup?id=1706428468&entity=song");
        assert_eq!(lookup_url("1706428468", "GB"), "https://itunes.apple.com/lookup?id=1706428468&entity=song&country=gb");
    }
}
