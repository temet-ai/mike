use std::sync::Mutex;
use std::time::Duration;

use super::model_ref::Provider;

#[cfg(test)]
#[path = "backoff_tests.rs"]
pub mod backoff_tests;

const BASE_DELAY: Duration = Duration::from_secs(2);
pub const MAX_DELAY: Duration = Duration::from_secs(60);
const TRANSPORTS: usize = 3;

#[derive(Default)]
pub struct TimeoutBackoff {
    streaks: Mutex<[u32; TRANSPORTS]>,
}

fn slot(provider: Provider) -> usize {
    match provider {
        Provider::OpenRouter => 0,
        Provider::Claude => 1,
        Provider::Codex => 2,
    }
}

pub fn delay_for(streak: u32) -> Duration {
    match streak {
        0 => Duration::ZERO,
        n => BASE_DELAY
            .checked_mul(1u32 << (n - 1).min(6))
            .map_or(MAX_DELAY, |d| d.min(MAX_DELAY)),
    }
}

impl TimeoutBackoff {
    pub fn pause(&self, provider: Provider) -> Duration {
        let streaks = self.streaks.lock().unwrap_or_else(|e| e.into_inner());
        delay_for(streaks[slot(provider)])
    }

    pub fn record(&self, provider: Provider, timed_out: bool) -> u32 {
        let mut streaks = self.streaks.lock().unwrap_or_else(|e| e.into_inner());
        let streak = &mut streaks[slot(provider)];
        *streak = if timed_out {
            streak.saturating_add(1)
        } else {
            0
        };
        *streak
    }
}

static GLOBAL: TimeoutBackoff = TimeoutBackoff {
    streaks: Mutex::new([0; TRANSPORTS]),
};

pub fn pause(provider: Provider) -> Duration {
    GLOBAL.pause(provider)
}

pub fn record(provider: Provider, timed_out: bool) -> u32 {
    GLOBAL.record(provider, timed_out)
}
