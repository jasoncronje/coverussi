//! IANA time zones to the POSIX rules the board's libc takes in `TZ`, and to
//! the country they're in.

use crate::zones::ZONES;

/// The POSIX TZ rule for an IANA zone, such as `GMT0BST,M3.5.0/1,M10.5.0` for Europe/London.
pub fn posix_tz(zone: &str) -> Option<&'static str> {
    find(zone).map(|z| z.1)
}

/// The two-letter country an IANA zone is in, such as `GB` for Europe/London; None for UTC and the like.
pub fn country(zone: &str) -> Option<&'static str> {
    find(zone).map(|z| z.2).filter(|c| !c.is_empty())
}

fn find(zone: &str) -> Option<&'static (&'static str, &'static str, &'static str)> {
    ZONES.binary_search_by(|z| z.0.cmp(zone)).ok().map(|i| &ZONES[i])
}

/// Every zone name, for the portal's picker.
pub fn names() -> impl Iterator<Item = &'static str> {
    ZONES.iter().map(|z| z.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_and_unknown_zones() {
        assert_eq!(posix_tz("Europe/London"), Some("GMT0BST,M3.5.0/1,M10.5.0"));
        assert_eq!(posix_tz("UTC"), Some("UTC0"));
        assert_eq!(posix_tz("Mars/Olympus"), None);
        assert_eq!(posix_tz("../etc/passwd"), None);
        assert!(names().count() > 500);
    }

    #[test]
    fn countries() {
        assert_eq!(country("America/Los_Angeles"), Some("US"));
        assert_eq!(country("Europe/London"), Some("GB"));
        assert_eq!(country("US/Pacific"), Some("US")); // a link
        assert_eq!(country("UTC"), None);
        assert_eq!(country("Mars/Olympus"), None);
    }

    #[test]
    fn table_is_sorted_for_binary_search() {
        assert!(ZONES.windows(2).all(|w| w[0].0 < w[1].0));
    }
}
