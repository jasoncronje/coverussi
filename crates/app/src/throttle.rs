//! Spaces Shazam requests and backs off after a refusal.
//!
//! Measured November 1, 2026: two requests in a row were fine, the third got
//! 429s for about 150 s; after that, one every 30 s went through. A record
//! changes track every few minutes, so the default spacing costs nothing.

pub struct Throttle {
    pub min_interval_s: f64,
    pub cooldown_s: f64,
    pub max_cooldown_s: f64,
    last_request: f64,
    blocked_until: f64,
    /// Refusals in a row.
    pub refusals: u32,
}

impl Default for Throttle {
    fn default() -> Self {
        Self::new(20.0, 60.0, 600.0)
    }
}

impl Throttle {
    pub fn new(min_interval_s: f64, cooldown_s: f64, max_cooldown_s: f64) -> Self {
        Self {
            min_interval_s,
            cooldown_s,
            max_cooldown_s,
            last_request: f64::NEG_INFINITY,
            blocked_until: f64::NEG_INFINITY,
            refusals: 0,
        }
    }

    /// How long until a request may go, at `now` (seconds on any steady clock).
    pub fn wait_ms(&self, now: f64) -> u32 {
        let ready = (self.last_request + self.min_interval_s).max(self.blocked_until);
        if ready > now { ((ready - now) * 1000.0).round() as u32 } else { 0 }
    }

    /// Claim a request at `now`, or say how long to wait.
    pub fn begin(&mut self, now: f64) -> Result<(), u32> {
        match self.wait_ms(now) {
            0 => {
                self.last_request = now;
                Ok(())
            }
            wait => Err(wait),
        }
    }

    /// Shazam refused (HTTP 429): the cooldown before the next try, in ms.
    pub fn refused(&mut self, now: f64) -> u32 {
        self.refusals += 1;
        let cooldown = (self.cooldown_s * 2f64.powi(self.refusals as i32 - 1)).min(self.max_cooldown_s);
        self.blocked_until = now + cooldown;
        (cooldown * 1000.0).round() as u32
    }

    /// Shazam answered.
    pub fn answered(&mut self) {
        self.refusals = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn spaces_requests() {
        let mut t = Throttle::new(20.0, 60.0, 600.0);
        assert_eq!(t.begin(0.0), Ok(()));
        t.answered();
        assert_eq!(t.begin(5.0), Err(15_000));
        assert_eq!(t.begin(20.0), Ok(()));
    }

    #[test]
    fn backs_off_after_refusals() {
        let mut t = Throttle::new(0.0, 60.0, 100.0);
        assert_eq!(t.begin(0.0), Ok(()));
        assert_eq!(t.refused(0.0), 60_000);
        assert_eq!(t.begin(30.0), Err(30_000));
        assert_eq!(t.begin(60.0), Ok(()));
        assert_eq!(t.refused(60.0), 100_000); // doubled, capped
        assert_eq!(t.begin(160.0), Ok(()));
        t.answered();
        assert_eq!(t.refusals, 0);
    }
}
