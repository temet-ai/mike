use std::time::Duration;

use tokio::time::Instant;

pub const SETTLE_MARGIN: Duration = Duration::from_secs(30);

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct SettleCheck {
    due: Option<Instant>,
    last_build: Option<Instant>,
}

impl SettleCheck {
    pub fn due(&self) -> Option<Instant> {
        self.due
    }

    pub fn note_build(&mut self, now: Instant, settle: Duration) {
        self.last_build = Some(now);
        self.due.get_or_insert(now + settle + SETTLE_MARGIN);
    }

    pub fn take_due(&mut self, now: Instant, settle: Duration) -> bool {
        if !self.due.is_some_and(|due| due <= now) {
            return false;
        }
        self.due = self
            .last_build
            .map(|built| built + settle + SETTLE_MARGIN)
            .filter(|next| *next > now);
        true
    }
}

#[cfg(test)]
#[path = "settle_check_tests.rs"]
mod tests;
