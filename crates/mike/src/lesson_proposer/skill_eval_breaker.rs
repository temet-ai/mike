use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) const TRIP_AFTER: u32 = 3;
pub(super) const PAUSE_SECS: u64 = 1800;

pub(super) struct Breaker {
    consecutive: AtomicU32,
    last_failure_secs: AtomicU64,
}

impl Breaker {
    pub(super) const fn new() -> Self {
        Self {
            consecutive: AtomicU32::new(0),
            last_failure_secs: AtomicU64::new(0),
        }
    }

    pub(super) fn allows(&self, now_secs: u64) -> bool {
        if self.consecutive.load(Ordering::Relaxed) < TRIP_AFTER {
            return true;
        }
        now_secs.saturating_sub(self.last_failure_secs.load(Ordering::Relaxed)) >= PAUSE_SECS
    }

    pub(super) fn succeeded(&self) {
        self.consecutive.store(0, Ordering::Relaxed);
    }

    pub(super) fn failed(&self, now_secs: u64) {
        self.consecutive.fetch_add(1, Ordering::Relaxed);
        self.last_failure_secs.store(now_secs, Ordering::Relaxed);
    }
}

pub(super) static LOOP: Breaker = Breaker::new();

pub(super) fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
