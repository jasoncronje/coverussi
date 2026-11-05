//! Names reduced to keys for comparing releases. These were regular expressions
//! in the hub; they're written out here to keep the regex crate off the board.

const EDITION_WORDS: [&str; 9] = [
    "remaster", "deluxe", "edition", "anniversary", "expanded", "bonus", "mono", "stereo", "version",
];

/// Letters, digits and `_` only, case-folded: what `\W+` removal and `casefold` left.
pub fn norm(s: &str) -> String {
    s.chars()
        .filter(|c| c.is_alphanumeric() || *c == '_')
        .flat_map(char::to_lowercase)
        .collect()
}

/// Artist and album with edition noise removed, for comparing releases.
pub fn album_key(artist: &str, album: Option<&str>) -> String {
    let name = strip_presents(&strip_edition(album.unwrap_or("")));
    norm(&format!("{artist}|{name}"))
}

/// The lead artist: "Daft Punk, Pharrell Williams & Nile Rodgers" → "daftpunk".
pub fn artist_key(artist: &str) -> String {
    let lower = artist.to_ascii_lowercase();
    let cut = [",", "&", " feat. ", " feat ", " ft. ", " ft ", " with "]
        .iter()
        .filter_map(|sep| lower.find(sep))
        .min()
        .unwrap_or(artist.len());
    norm(&artist[..cut])
}

pub fn title_key(title: &str) -> String {
    norm(&strip_edition(strip_dash_suffix(title)))
}

/// Drops "(Super Deluxe Edition)", "[2011 Remaster]" and the like, with the
/// space before them: a bracketed group naming an edition.
fn strip_edition(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(open) = rest.find(['(', '[']) {
        let after = &rest[open + 1..];
        let Some(close) = after.find([')', ']']) else { break };
        let inside = after[..close].to_lowercase();
        let (before, group_end) = (&rest[..open], open + 1 + close + 1);
        if EDITION_WORDS.iter().any(|w| inside.contains(w)) {
            out.push_str(before.trim_end());
        } else {
            out.push_str(&rest[..group_end]);
        }
        rest = &rest[group_end..];
    }
    out.push_str(rest);
    out
}

/// "BD Music Presents Kind of Blue" → "Kind of Blue": everything up to the last
/// word "presents" and the space after it.
fn strip_presents(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut keep_from = None;
    for (i, _) in s.char_indices() {
        let word = &bytes[i..];
        if word.len() < 9 || !word[..8].eq_ignore_ascii_case(b"presents") {
            continue;
        }
        let starts_word = s[..i].chars().next_back().is_none_or(|c| !(c.is_alphanumeric() || c == '_'));
        let tail = &s[i + 8..];
        let spaces = tail.len() - tail.trim_start().len();
        if starts_word && spaces > 0 {
            keep_from = Some(i + 8 + spaces);
        }
    }
    keep_from.map_or_else(|| s.to_string(), |k| s[k..].to_string())
}

/// "Dreams - 2004 Remaster" → "Dreams": from the first dash with spaces around it.
fn strip_dash_suffix(s: &str) -> &str {
    for (i, c) in s.char_indices() {
        if c != '-' || i == 0 {
            continue;
        }
        let before = s[..i].chars().next_back().is_some_and(char::is_whitespace);
        let after = s[i + 1..].chars().next().is_some_and(char::is_whitespace);
        if before && after {
            return s[..i].trim_end();
        }
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_ignore_edition_noise() {
        assert_eq!(album_key("Fleetwood Mac", Some("Rumours")), album_key("Fleetwood Mac", Some("Rumours (Super Deluxe Edition)")));
        assert_eq!(album_key("Miles Davis", Some("BD Music Presents Kind of Blue")), album_key("Miles Davis", Some("Kind of Blue")));
        assert_eq!(artist_key("Daft Punk, Pharrell Williams & Nile Rodgers"), "daftpunk");
    }

    #[test]
    fn other_brackets_and_dashes() {
        assert_eq!(album_key("A", Some("Live (At Leeds)")), "aliveatleeds");
        assert_eq!(album_key("A", Some("Abbey Road [2019 Mix] (Remastered)")), "aabbeyroad2019mix");
        assert_eq!(title_key("Dreams - 2004 Remaster"), "dreams");
        assert_eq!(title_key("Get-Up"), "getup");
        assert_eq!(title_key("Sara (2015 Remaster)"), "sara");
        assert_eq!(artist_key("Calvin Harris feat. Rihanna"), "calvinharris");
        assert_eq!(artist_key("Simon With Garfunkel"), "simon");
        assert_eq!(album_key("A", Some("Representsation")), "arepresentsation");
        assert_eq!(album_key("Björk", None), "björk");
    }
}
