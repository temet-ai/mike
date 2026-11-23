use chrono::{DateTime, Duration, Utc};

use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fresh_marks::FreshTicket;

pub(super) const CLAIM_BACKSTOP: Duration = Duration::seconds(600);
pub(super) const IMMEDIATE_SETTLE: Duration = Duration::seconds(300);
const NOTHING_TO_CLAIM: &str = "live_judge.nothing_to_claim";
const JUDGE_QUEUE: [RowChangedTable; 1] = [RowChangedTable::JudgeQueue];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct EmptyClaim {
    bundle_id: i64,
    until: DateTime<Utc>,
    changed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Default)]
pub(super) struct ClaimGate {
    empty: Option<EmptyClaim>,
}

impl ClaimGate {
    pub(super) fn should_claim(
        &mut self,
        bundle_id: i64,
        unchanged: bool,
        settle: Duration,
        now: DateTime<Utc>,
    ) -> bool {
        let Some(empty) = self.empty.as_mut() else {
            return true;
        };
        if empty.bundle_id != bundle_id || now >= empty.until {
            return true;
        }
        if unchanged {
            return false;
        }
        let changed_at = *empty.changed_at.get_or_insert(now);
        changed_at + settle <= now
    }

    pub(super) fn settle(&mut self, bundle_id: i64, found: usize, fed: bool, now: DateTime<Utc>) {
        self.empty = (found == 0 && fed).then_some(EmptyClaim {
            bundle_id,
            until: now + CLAIM_BACKSTOP,
            changed_at: None,
        });
    }
}

pub(super) fn settle_for(immediate: bool) -> Duration {
    if immediate {
        IMMEDIATE_SETTLE
    } else {
        Duration::zero()
    }
}

pub(super) fn queue_unchanged(persist: &PersistHandle, now: DateTime<Utc>) -> bool {
    persist.is_fresh(NOTHING_TO_CLAIM, &JUDGE_QUEUE, now)
}

pub(super) fn note_claim(persist: &PersistHandle, ticket: Option<FreshTicket>, found: usize) {
    if found == 0 {
        persist.mark_fresh(NOTHING_TO_CLAIM, ticket, None);
    } else {
        persist.forget_fresh(NOTHING_TO_CLAIM);
    }
}

#[cfg(test)]
#[path = "claim_gate_tests.rs"]
mod claim_gate_tests;
