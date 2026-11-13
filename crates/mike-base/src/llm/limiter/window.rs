use std::time::{Duration, Instant};

use crate::llm::budget::{RoleShares, calls_available};
use crate::llm::usage::{LlmRole, ROLE_COUNT};

#[cfg(any(test, feature = "test-seams"))]
const EXHAUSTED_USED: u32 = u32::MAX / 8;

pub struct RoleWindow {
    window: Duration,
    limit: u32,
    start: Instant,
    epoch: u64,
    used: [u32; ROLE_COUNT],
}

impl RoleWindow {
    pub fn new(limit: u32, window: Duration, now: Instant) -> Self {
        Self {
            window,
            limit,
            start: now,
            epoch: 0,
            used: [0; ROLE_COUNT],
        }
    }

    pub fn tune(&mut self, limit: u32, window: Duration) {
        self.limit = limit;
        self.window = window;
    }

    pub fn roll_if_elapsed(&mut self, now: Instant) {
        if now.duration_since(self.start) >= self.window {
            self.start = now;
            self.epoch += 1;
            self.used = [0; ROLE_COUNT];
        }
    }

    pub fn available(&self, role: LlmRole, shares: &RoleShares) -> u32 {
        calls_available(self.limit, shares, &self.used, role)
    }

    pub fn charge(&mut self, role: LlmRole, calls: u32) {
        if let Some(slot) = self.used.get_mut(role.index()) {
            *slot = slot.saturating_add(calls);
        }
    }

    pub fn release(&mut self, role: LlmRole, calls: u32, epoch: u64) {
        if epoch != self.epoch {
            return;
        }
        if let Some(slot) = self.used.get_mut(role.index()) {
            *slot = slot.saturating_sub(calls);
        }
    }

    pub fn remaining(&self, now: Instant) -> Duration {
        self.window.saturating_sub(now.duration_since(self.start))
    }

    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    pub fn limit(&self) -> u32 {
        self.limit
    }

    pub fn window(&self) -> Duration {
        self.window
    }

    pub fn used(&self) -> [u32; ROLE_COUNT] {
        self.used
    }

    pub fn total_used(&self) -> u32 {
        self.used.iter().sum()
    }

    #[cfg(any(test, feature = "test-seams"))]
    pub fn reset(&mut self, now: Instant) {
        self.start = now;
        self.epoch += 1;
        self.used = [0; ROLE_COUNT];
    }

    #[cfg(any(test, feature = "test-seams"))]
    pub fn exhaust(&mut self) {
        self.used = [EXHAUSTED_USED; ROLE_COUNT];
    }
}
