use std::time::{Duration, Instant};

use crate::llm::budget::{RoleShares, calls_available};
use crate::llm::usage::{LlmRole, ROLE_COUNT};

#[cfg(any(test, feature = "test-seams"))]
const EXHAUSTED_USED: u32 = u32::MAX / 8;

pub(super) struct RoleWindow {
    window: Duration,
    limit: u32,
    start: Instant,
    epoch: u64,
    used: [u32; ROLE_COUNT],
}

impl RoleWindow {
    pub(super) fn new(limit: u32, window: Duration, now: Instant) -> Self {
        Self {
            window,
            limit,
            start: now,
            epoch: 0,
            used: [0; ROLE_COUNT],
        }
    }

    pub(super) fn tune(&mut self, limit: u32, window: Duration) {
        self.limit = limit;
        self.window = window;
    }

    pub(super) fn roll_if_elapsed(&mut self, now: Instant) {
        if now.duration_since(self.start) >= self.window {
            self.start = now;
            self.epoch += 1;
            self.used = [0; ROLE_COUNT];
        }
    }

    pub(super) fn available(&self, role: LlmRole, shares: &RoleShares) -> u32 {
        calls_available(self.limit, shares, &self.used, role)
    }

    pub(super) fn charge(&mut self, role: LlmRole, calls: u32) {
        if let Some(slot) = self.used.get_mut(role.index()) {
            *slot = slot.saturating_add(calls);
        }
    }

    pub(super) fn release(&mut self, role: LlmRole, calls: u32, epoch: u64) {
        if epoch != self.epoch {
            return;
        }
        if let Some(slot) = self.used.get_mut(role.index()) {
            *slot = slot.saturating_sub(calls);
        }
    }

    pub(super) fn remaining(&self, now: Instant) -> Duration {
        self.window.saturating_sub(now.duration_since(self.start))
    }

    pub(super) fn epoch(&self) -> u64 {
        self.epoch
    }

    pub(super) fn limit(&self) -> u32 {
        self.limit
    }

    pub(super) fn window(&self) -> Duration {
        self.window
    }

    pub(super) fn used(&self) -> [u32; ROLE_COUNT] {
        self.used
    }

    pub(super) fn total_used(&self) -> u32 {
        self.used.iter().sum()
    }

    #[cfg(any(test, feature = "test-seams"))]
    pub(super) fn reset(&mut self, now: Instant) {
        self.start = now;
        self.epoch += 1;
        self.used = [0; ROLE_COUNT];
    }

    #[cfg(any(test, feature = "test-seams"))]
    pub(super) fn exhaust(&mut self) {
        self.used = [EXHAUSTED_USED; ROLE_COUNT];
    }
}
