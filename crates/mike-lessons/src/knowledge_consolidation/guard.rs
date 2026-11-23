use super::llm::Failure;
use crate::circuit_breaker::CircuitBreaker;
use chrono::{DateTime, Duration, Utc};
use std::collections::HashMap;
use std::time::{Duration as StdDuration, Instant};

const BREAKER_THRESHOLD: u32 = 3;
const BREAKER_COOLDOWN_BASE: StdDuration = StdDuration::from_secs(30);
const BREAKER_COOLDOWN_MAX: StdDuration = StdDuration::from_secs(600);
const RETRY_BASE_SECS: i64 = 60;
const RETRY_MAX_SECS: i64 = 3_600;
const PERMANENT_RETRY_SECS: i64 = 6 * 3_600;

pub struct Guard {
    breaker: CircuitBreaker,
    failures: HashMap<(char, i64), u32>,
}

impl Default for Guard {
    fn default() -> Self {
        Self::new()
    }
}

impl Guard {
    pub fn new() -> Self {
        Self {
            breaker: CircuitBreaker::new(
                BREAKER_THRESHOLD,
                BREAKER_COOLDOWN_BASE,
                BREAKER_COOLDOWN_MAX,
            ),
            failures: HashMap::new(),
        }
    }

    pub fn open(&self, now: Instant) -> bool {
        self.breaker.should_skip(now)
    }

    pub fn succeeded(&mut self, now: Instant) {
        self.breaker.record(true, now);
    }

    pub fn failed(&mut self, failure: &Failure, now: Instant) {
        if failure.transient {
            self.breaker.record(false, now);
        }
    }

    pub fn retry_at(&mut self, key: (char, i64), failure: &Failure, now: DateTime<Utc>) -> DateTime<Utc> {
        if !failure.transient {
            return now + Duration::seconds(PERMANENT_RETRY_SECS);
        }
        let count = self.failures.entry(key).or_insert(0);
        *count = count.saturating_add(1);
        let exponent = (*count - 1).min(10);
        let secs = (RETRY_BASE_SECS << exponent).min(RETRY_MAX_SECS);
        now + Duration::seconds(secs)
    }

    pub fn forget(&mut self, key: (char, i64)) {
        self.failures.remove(&key);
    }
}
