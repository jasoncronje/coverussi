//! Firmware versions: the board's against the latest GitHub release.

/// The numbers of "v1.2.3" or "1.2.3-dev"; a pre-release suffix sorts before
/// the release it leads to.
fn parts(v: &str) -> Option<([u64; 3], bool)> {
    let v = v.trim().trim_start_matches('v');
    let (core, pre) = match v.split_once('-') {
        Some((core, _)) => (core, true),
        None => (v, false),
    };
    let mut n = [0u64; 3];
    let mut it = core.split('.');
    for slot in &mut n {
        *slot = it.next()?.parse().ok()?;
    }
    it.next().is_none().then_some((n, pre))
}

/// Whether `latest` (a release tag) is newer than `current`. Anything that
/// isn't a version counts as not newer.
pub fn newer(latest: &str, current: &str) -> bool {
    match (parts(latest), parts(current)) {
        (Some((l, l_pre)), Some((c, c_pre))) => l > c || (l == c && c_pre && !l_pre),
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compares_release_tags_with_the_board() {
        assert!(newer("v0.1.1", "0.1.0"));
        assert!(newer("v0.2.0", "0.1.9"));
        assert!(newer("v1.0.0", "0.10.0"));
        assert!(newer("v0.10.0", "0.9.0")); // numbers, not text
        assert!(!newer("v0.1.0", "0.1.0"));
        assert!(!newer("v0.1.0", "0.1.1")); // never offers an older one
        assert!(newer("v0.1.0", "0.1.0-dev")); // a dev build moves on to its release
        assert!(!newer("v0.1.0-beta", "0.1.0"));
        assert!(!newer("latest", "0.1.0"));
        assert!(!newer("v0.1", "0.1.0"));
    }
}
