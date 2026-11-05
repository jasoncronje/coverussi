//! The web app's login: one password, stored as a salted hash, and a signed
//! session cookie that needs nothing kept on the board.

use hmac::{Hmac, Mac};
use sha2::Sha256;

pub const COOKIE: &str = "coverussi_session";
pub const SESSION_S: u64 = 30 * 24 * 3600;
pub const MIN_PASSWORD: usize = 8;
const ROUNDS: u32 = 2000;

fn mac(key: &[u8], parts: &[&[u8]]) -> [u8; 32] {
    let mut m = Hmac::<Sha256>::new_from_slice(key).expect("any key length");
    for p in parts {
        m.update(p);
    }
    m.finalize().into_bytes().into()
}

fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}

/// What to store for a password: the salt and the hash, hex.
pub fn hash_password(password: &str, salt: &[u8; 16]) -> String {
    let mut h = mac(salt, &[password.as_bytes()]);
    for _ in 1..ROUNDS {
        h = mac(salt, &[&h, password.as_bytes()]);
    }
    format!("{}${}", hex(salt), hex(&h))
}

pub fn password_ok(stored: &str, given: &str) -> bool {
    let Some((salt, _)) = stored.split_once('$') else { return false };
    let Some(salt) = unhex(salt).and_then(|s| <[u8; 16]>::try_from(s).ok()) else { return false };
    same(hash_password(given, &salt).as_bytes(), stored.as_bytes())
}

fn unhex(s: &str) -> Option<Vec<u8>> {
    (0..s.len()).step_by(2).map(|i| u8::from_str_radix(s.get(i..i + 2)?, 16).ok()).collect()
}

/// Compares in the same time wherever they differ.
fn same(a: &[u8], b: &[u8]) -> bool {
    a.len() == b.len() && a.iter().zip(b).fold(0, |acc, (x, y)| acc | (x ^ y)) == 0
}

/// Before this, the board's clock hasn't been set.
const CLOCK_SET: u64 = 1_600_000_000;

/// A session cookie's value, good until `now + SESSION_S`. Before the clock is
/// set it's good until the board restarts: `boot` is random each start.
pub fn new_session(secret: &[u8], boot: &[u8], now: u64) -> String {
    if now < CLOCK_SET { sign(&boot_key(secret, boot), 0) } else { sign(secret, now + SESSION_S) }
}

fn boot_key(secret: &[u8], boot: &[u8]) -> [u8; 32] {
    mac(secret, &[b"boot", boot])
}

fn sign(secret: &[u8], expires: u64) -> String {
    let e = expires.to_string();
    format!("{e}.{}", hex(&mac(secret, &[e.as_bytes()])))
}

pub fn valid_session(secret: &[u8], boot: &[u8], value: &str, now: u64) -> bool {
    let Some((e, _)) = value.split_once('.') else { return false };
    let Ok(expires) = e.parse::<u64>() else { return false };
    if expires == 0 {
        return same(sign(&boot_key(secret, boot), 0).as_bytes(), value.as_bytes());
    }
    expires >= now && same(sign(secret, expires).as_bytes(), value.as_bytes())
}

/// The session from a Cookie header.
pub fn cookie(header: &str) -> Option<&str> {
    header.split(';').find_map(|c| c.trim().strip_prefix(COOKIE)?.strip_prefix('='))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn passwords() {
        let stored = hash_password("correct horse", &[7; 16]);
        assert!(stored.starts_with("07070707"));
        assert!(password_ok(&stored, "correct horse"));
        assert!(!password_ok(&stored, "correct horsf"));
        assert!(!password_ok("", "x") && !password_ok("zz$00", "x"));
        assert_ne!(hash_password("correct horse", &[8; 16]), stored);
    }

    #[test]
    fn sessions() {
        const NOW: u64 = 1_800_000_000;
        let s = new_session(b"secret", b"boot", NOW);
        assert!(valid_session(b"secret", b"boot", &s, NOW + SESSION_S));
        assert!(valid_session(b"secret", b"later", &s, NOW)); // survives a restart
        assert!(valid_session(b"secret", b"boot", &s, 5)); // and a restart before the clock is set
        assert!(!valid_session(b"secret", b"boot", &s, NOW + 1 + SESSION_S));
        assert!(!valid_session(b"other", b"boot", &s, NOW));
        let forged = format!("{}{}", &s[..s.len() - 1], if s.ends_with('0') { '1' } else { '0' });
        assert!(!valid_session(b"secret", b"boot", &forged, NOW));
        assert!(!valid_session(b"secret", b"boot", "9999999999999.ab", NOW));
    }

    #[test]
    fn sessions_before_the_clock_is_set() {
        let s = new_session(b"secret", b"boot", 12);
        assert!(s.starts_with("0."));
        assert!(valid_session(b"secret", b"boot", &s, 12));
        assert!(valid_session(b"secret", b"boot", &s, 1_800_000_000)); // the clock got set
        assert!(!valid_session(b"secret", b"next boot", &s, 1_800_000_000));
        assert!(!valid_session(b"other", b"boot", &s, 12));
        assert_eq!(cookie(&format!("a=1; {COOKIE}={s}; b=2")), Some(s.as_str()));
        assert_eq!(cookie("a=1"), None);
    }
}
