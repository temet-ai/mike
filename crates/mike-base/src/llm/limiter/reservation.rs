use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

use super::CallLimiter;
use crate::llm::usage::LlmRole;

pub struct Held {
    limiter: Arc<CallLimiter>,
    role: LlmRole,
    granted: u32,
    remaining: AtomicU32,
    epochs: (u64, u64),
    next_slot_in: Duration,
}

impl Drop for Held {
    fn drop(&mut self) {
        let unused = *self.remaining.get_mut();
        if unused > 0 {
            self.limiter.release(self.role, unused, self.epochs);
        }
    }
}

#[derive(Clone)]
pub struct Reservation {
    held: Arc<Held>,
}

impl Reservation {
    pub fn new(
        limiter: Arc<CallLimiter>,
        role: LlmRole,
        granted: u32,
        epochs: (u64, u64),
        next_slot_in: Duration,
    ) -> Self {
        Self {
            held: Arc::new(Held {
                limiter,
                role,
                granted,
                remaining: AtomicU32::new(granted),
                epochs,
                next_slot_in,
            }),
        }
    }

    pub fn granted(&self) -> u32 {
        self.held.granted
    }

    #[cfg(test)]
    pub fn remaining(&self) -> u32 {
        self.held.remaining.load(Ordering::Relaxed)
    }

    pub fn next_slot_in(&self) -> Duration {
        self.held.next_slot_in
    }

    pub fn consume(&self, role: LlmRole, epochs: (u64, u64)) -> bool {
        if self.held.role != role || self.held.epochs != epochs {
            return false;
        }
        self.held
            .remaining
            .fetch_update(Ordering::AcqRel, Ordering::Acquire, |left| {
                left.checked_sub(1)
            })
            .is_ok()
    }
}
