//! Shazam's tag endpoint, as shazamio uses it: the request for a signature and
//! the match in its answer. The board sends it with its own HTTP client.

use serde_json::{json, Value};

use crate::Match;

/// Art size in pixels; the panel scales down, Home Assistant shows it as is.
pub const ART_SIZE: u32 = 600;

/// The `discovery/v5` tag URL. The device segment is `iphone` because the `web`
/// profile leaves out `hub.providers`. `uuid_1` and `uuid_2` are fresh
/// upper-case UUIDs per request.
pub fn tag_url(uuid_1: &str, uuid_2: &str) -> String {
    format!(
        "https://amp.shazam.com/discovery/v5/en-US/GB/iphone/-/tag/{uuid_1}/{uuid_2}\
         ?sync=true&webv3=true&sampling=true&connected=&shazamapiversion=v3&sharehub=true\
         &hubv5minorversion=v5.1&hidelb=true&video=v3"
    )
}

pub const HEADERS: [(&str, &str); 6] = [
    ("Content-Type", "application/json"),
    ("X-Shazam-Platform", "IPHONE"),
    ("X-Shazam-AppVersion", "14.1.0"),
    ("Accept", "*/*"),
    ("Accept-Language", "en-US"),
    ("User-Agent", "Dalvik/2.1.0 (Linux; U; Android 5.0.2; VS980 4G Build/LRX22G)"),
];

/// The JSON body for a signature; `timestamp_ms` is the epoch time.
pub fn tag_body(uri: &str, sample_ms: u32, timestamp_ms: u64) -> String {
    json!({
        "timezone": "Europe/Paris",
        "signature": {"uri": uri, "samplems": sample_ms},
        "timestamp": timestamp_ms,
        "context": {},
        "geolocation": {},
    })
    .to_string()
}

/// Apple's image URLs end in a size such as `400x400cc.jpg`; ask for `size`.
pub fn resize_art(url: Option<&str>, size: u32) -> Option<String> {
    let url = url.filter(|u| !u.is_empty())?;
    let Some(slash) = url.rfind('/') else { return Some(url.to_string()) };
    let (head, name) = url.split_at(slash + 1);
    let resized = (|| {
        let (dims, ext) = name.rsplit_once('.')?;
        if !matches!(ext, "jpg" | "png" | "webp") {
            return None;
        }
        let (w, rest) = dims.split_once('x')?;
        let digits = rest.find(|c: char| !c.is_ascii_digit()).unwrap_or(rest.len());
        let (h, suffix) = rest.split_at(digits);
        let all_digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
        let word = suffix.chars().all(|c| c.is_alphanumeric() || c == '_');
        (all_digits(w) && all_digits(h) && word).then(|| format!("{head}{size}x{size}{suffix}.{ext}"))
    })();
    Some(resized.unwrap_or_else(|| url.to_string()))
}

/// The match in a Shazam answer, or None when it found nothing.
pub fn parse(raw: &Value) -> Option<Match> {
    let first = raw.get("matches")?.as_array()?.first()?;
    let track = raw.get("track").filter(|t| t.is_object())?;
    let text = |v: &Value, k: &str| v.get(k).and_then(Value::as_str).map(String::from);

    let song_meta = track
        .get("sections")
        .and_then(Value::as_array)
        .and_then(|s| s.iter().find(|s| s.get("type").and_then(Value::as_str) == Some("SONG")))
        .and_then(|s| s.get("metadata"))
        .and_then(Value::as_array);
    let meta = |title: &str| {
        song_meta?
            .iter()
            .find(|m| m.get("title").and_then(Value::as_str) == Some(title))
            .and_then(|m| text(m, "text"))
    };
    let apple_id = track
        .pointer("/hub/actions")
        .and_then(Value::as_array)
        .and_then(|a| a.iter().find(|a| a.get("type").and_then(Value::as_str) == Some("applemusicplay")))
        .and_then(|a| a.get("id"))
        .and_then(|id| match id {
            Value::String(s) if !s.is_empty() => Some(s.clone()),
            Value::Number(n) => Some(n.to_string()),
            _ => None,
        });
    let image = |k: &str| track.pointer(&format!("/images/{k}")).and_then(Value::as_str);
    let num = |k: &str| first.get(k).and_then(Value::as_f64).unwrap_or(0.0);

    Some(Match {
        title: text(track, "title").unwrap_or_default(),
        artist: text(track, "subtitle").unwrap_or_default(),
        album: meta("Album"),
        year: meta("Released"),
        label: meta("Label"),
        isrc: text(track, "isrc"),
        shazam_key: text(track, "key"),
        apple_id,
        collection_id: None,
        art_url: resize_art(image("coverarthq").or(image("coverart")), ART_SIZE),
        offset_ms: (num("offset") * 1000.0).round().max(0.0) as u32,
        duration_ms: None,
        skew: num("timeskew").abs().max(num("frequencyskew").abs()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn peg() -> Value {
        serde_json::from_str(include_str!("../tests/fixtures/shazam_peg.json")).unwrap()
    }

    #[test]
    fn parse_shazam_answer() {
        let m = parse(&peg()).unwrap();
        assert_eq!(
            (m.title.as_str(), m.artist.as_str(), m.album.as_deref(), m.year.as_deref(), m.label.as_deref()),
            ("Peg", "Steely Dan", Some("Aja"), Some("1977"), Some("Geffen"))
        );
        assert_eq!(m.apple_id.as_deref(), Some("1706428468"));
        assert_eq!(m.isrc.as_deref(), Some("USMC17647230"));
        assert_eq!(m.offset_ms, 44632);
        assert!(m.art_url.as_deref().unwrap().ends_with("/600x600cc.jpg"));
        assert!((m.skew - 0.0012074709).abs() < 1e-9);
        assert_eq!(m.duration_ms, None);
    }

    #[test]
    fn parse_no_match() {
        assert_eq!(parse(&json!({"matches": [], "timestamp": 1})), None);
        assert_eq!(parse(&json!({})), None);
    }

    #[test]
    fn resize() {
        assert_eq!(resize_art(Some("https://x/a/100x100bb.jpg"), 600).as_deref(), Some("https://x/a/600x600bb.jpg"));
        assert_eq!(resize_art(Some("https://x/a/400x400cc.jpg"), 128).as_deref(), Some("https://x/a/128x128cc.jpg"));
        assert_eq!(resize_art(Some("https://x/a/cover.jpg"), 600).as_deref(), Some("https://x/a/cover.jpg"));
        assert_eq!(resize_art(None, 600), None);
    }

    #[test]
    fn body_is_what_shazamio_sends() {
        let body: Value = serde_json::from_str(&tag_body("data:x", 12000, 5)).unwrap();
        assert_eq!(body["signature"]["samplems"], 12000);
        assert_eq!(body["timestamp"], 5);
        assert!(tag_url("A", "B").contains("/tag/A/B?sync=true"));
    }
}
