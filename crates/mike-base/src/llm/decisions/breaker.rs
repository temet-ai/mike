use std::sync::Mutex;
use std::time::{Duration, Instant};

pub const OPEN_AFTER: u32 = 5;
pub const FIRST_COOLDOWN: Duration = Duration::from_secs(30);
pub const MAX_COOLDOWN: Duration = Duration::from_secs(15 * 60);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gate {
    Closed,
    Probe,
    Open { retry_in: Duration },
}

#[derive(Debug)]
pub struct Breaker {
    state: Mutex<State>,
}

#[derive(Debug, Default)]
struct State {
    failures: u32,
    cooldown: Duration,
    open_until: Option<Instant>,
    probing: bool,
}

impl Default for Breaker {
    fn default() -> Self {
        Self::new()
    }
}

impl Breaker {
    pub const fn new() -> Self {
        Self {
            state: Mutex::new(State {
                failures: 0,
                cooldown: Duration::ZERO,
                open_until: None,
                probing: false,
            }),
        }
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, State> {
        self.state
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }

    pub fn gate(&self, now: Instant) -> Gate {
        let mut s = self.lock();
        match s.open_until {
            None => Gate::Closed,
            Some(until) if now < until => Gate::Open {
                retry_in: until - now,
            },
            Some(_) if s.probing => Gate::Open {
                retry_in: Duration::ZERO,
            },
            Some(_) => {
                s.probing = true;
                Gate::Probe
            }
        }
    }

    pub fn succeeded(&self) {
        let mut s = self.lock();
        if s.open_until.is_some() {
            tracing::info!("decisions: Jev answered again, the breaker is closed");
        }
        *s = State::default();
    }

    pub fn failed(&self, now: Instant) {
        let mut s = self.lock();
        s.failures = s.failures.saturating_add(1);
        s.probing = false;
        if s.failures < OPEN_AFTER {
            return;
        }
        s.cooldown = if s.cooldown.is_zero() {
            FIRST_COOLDOWN
        } else {
            (s.cooldown * 2).min(MAX_COOLDOWN)
        };
        s.open_until = Some(now + s.cooldown);
        tracing::warn!(
            failures = s.failures,
            cooldown_secs = s.cooldown.as_secs(),
            "decisions: Jev failed {} times in a row; every point takes its off path until the breaker probes again",
            s.failures
        );
    }
}

pub static SHARED: Breaker = Breaker::new();

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn breaker_opens_after_five_failures_and_probes_once_after_the_cooldown() {
        let b = Breaker::new();
        let t0 = Instant::now();
        for _ in 0..4 {
            b.failed(t0);
        }
        assert_eq!(b.gate(t0), Gate::Closed);
        b.failed(t0);
        assert!(matches!(b.gate(t0), Gate::Open { .. }));
        let later = t0 + FIRST_COOLDOWN + Duration::from_millis(1);
        assert_eq!(b.gate(later), Gate::Probe);
        assert!(matches!(b.gate(later), Gate::Open { .. }));
    }

    #[test]
    fn breaker_failed_probe_doubles_the_cooldown_and_success_closes_it() {
        let b = Breaker::new();
        let t0 = Instant::now();
        for _ in 0..OPEN_AFTER {
            b.failed(t0);
        }
        let t1 = t0 + FIRST_COOLDOWN + Duration::from_millis(1);
        assert_eq!(b.gate(t1), Gate::Probe);
        b.failed(t1);
        let still = t1 + FIRST_COOLDOWN + Duration::from_millis(1);
        assert!(matches!(b.gate(still), Gate::Open { .. }));
        let t2 = t1 + FIRST_COOLDOWN * 2 + Duration::from_millis(1);
        assert_eq!(b.gate(t2), Gate::Probe);
        b.succeeded();
        assert_eq!(b.gate(t2), Gate::Closed);
    }
}
