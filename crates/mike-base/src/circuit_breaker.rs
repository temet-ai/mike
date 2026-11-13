use std::time::{Duration, Instant};

pub struct CircuitBreaker {
    consecutive_failures: u32,
    threshold: u32,
    cooldown: Duration,
    base_cooldown: Duration,
    max_cooldown: Duration,
    open_until: Option<Instant>,
}

impl CircuitBreaker {
    pub fn new(threshold: u32, base_cooldown: Duration, max_cooldown: Duration) -> Self {
        Self {
            consecutive_failures: 0,
            threshold: threshold.max(1),
            cooldown: base_cooldown,
            base_cooldown,
            max_cooldown,
            open_until: None,
        }
    }

    pub fn should_skip(&self, now: Instant) -> bool {
        matches!(self.open_until, Some(until) if now < until)
    }

    pub fn is_probing(&self, now: Instant) -> bool {
        matches!(self.open_until, Some(until) if now >= until)
    }

    pub fn record(&mut self, succeeded: bool, now: Instant) {
        if succeeded {
            self.consecutive_failures = 0;
            self.cooldown = self.base_cooldown;
            self.open_until = None;
            return;
        }
        self.consecutive_failures += 1;
        if self.consecutive_failures >= self.threshold {
            self.open_until = Some(now + self.cooldown);
            self.cooldown = (self.cooldown * 2).min(self.max_cooldown);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stays_closed_below_threshold() {
        let mut b = CircuitBreaker::new(3, Duration::from_secs(10), Duration::from_secs(60));
        let now = Instant::now();
        b.record(false, now);
        b.record(false, now);
        assert!(
            !b.should_skip(now),
            "two failures under a threshold of 3 must not trip the breaker"
        );
    }

    #[test]
    fn opens_at_threshold_and_skips_during_cooldown() {
        let mut b = CircuitBreaker::new(3, Duration::from_secs(10), Duration::from_secs(60));
        let t0 = Instant::now();
        b.record(false, t0);
        b.record(false, t0);
        b.record(false, t0);
        assert!(
            b.should_skip(t0),
            "the third consecutive failure must trip the breaker immediately"
        );
        let mid_cooldown = t0 + Duration::from_secs(5);
        assert!(
            b.should_skip(mid_cooldown),
            "still inside the 10s cooldown — must keep skipping"
        );
    }

    #[test]
    fn probes_after_cooldown_elapses() {
        let mut b = CircuitBreaker::new(2, Duration::from_secs(10), Duration::from_secs(60));
        let t0 = Instant::now();
        b.record(false, t0);
        b.record(false, t0);
        let after_cooldown = t0 + Duration::from_secs(11);
        assert!(
            !b.should_skip(after_cooldown),
            "cooldown elapsed — the breaker must allow a probe through"
        );
        assert!(
            b.is_probing(after_cooldown),
            "the breaker must self-identify as probing, not fully closed"
        );
    }

    #[test]
    fn closes_on_probe_success() {
        let mut b = CircuitBreaker::new(2, Duration::from_secs(10), Duration::from_secs(60));
        let t0 = Instant::now();
        b.record(false, t0);
        b.record(false, t0);
        let probe_time = t0 + Duration::from_secs(11);
        b.record(true, probe_time);
        assert!(
            !b.should_skip(probe_time),
            "a successful probe must close the breaker immediately"
        );
        assert!(!b.is_probing(probe_time), "a closed breaker is not probing");
        b.record(false, probe_time);
        assert!(
            !b.should_skip(probe_time),
            "one failure after a close must not immediately re-trip a threshold-2 breaker"
        );
    }

    #[test]
    fn doubles_cooldown_on_repeated_probe_failure() {
        let mut b = CircuitBreaker::new(1, Duration::from_secs(10), Duration::from_secs(60));
        let t0 = Instant::now();
        b.record(false, t0);
        let first_probe = t0 + Duration::from_secs(10);
        assert!(b.is_probing(first_probe));
        b.record(false, first_probe);
        assert!(
            b.should_skip(first_probe + Duration::from_secs(15)),
            "15s after the failed probe must still be inside the doubled ~20s cooldown"
        );
        assert!(
            !b.should_skip(first_probe + Duration::from_secs(21)),
            "21s after the failed probe must be past the doubled ~20s cooldown"
        );
    }

    #[test]
    fn cooldown_never_exceeds_max() {
        let mut b = CircuitBreaker::new(1, Duration::from_secs(10), Duration::from_secs(25));
        let mut now = Instant::now();
        for _ in 0..6 {
            b.record(false, now);
            now += Duration::from_secs(30);
        }
        assert!(
            b.cooldown <= Duration::from_secs(25),
            "cooldown must never exceed max_cooldown, got {:?}",
            b.cooldown
        );
    }

    #[test]
    fn new_floors_zero_threshold_at_one() {
        let mut b = CircuitBreaker::new(0, Duration::from_secs(10), Duration::from_secs(60));
        let now = Instant::now();
        assert!(!b.should_skip(now), "a fresh breaker must start closed");
        b.record(false, now);
        assert!(
            b.should_skip(now),
            "a floored threshold of 1 must trip on the first failure"
        );
    }
}
