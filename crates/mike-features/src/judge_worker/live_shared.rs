use std::sync::Arc;
use std::sync::atomic::{AtomicI64, Ordering};

use dashmap::DashMap;

#[derive(Debug, Clone, Default)]
pub struct LiveJudgeShared {
    in_flight: Arc<DashMap<i64, ()>>,
    announced_bundle: Arc<AtomicI64>,
    queue_reconciled_bundle: Arc<AtomicI64>,
    queue_reconciled_at_unix: Arc<AtomicI64>,
}

pub struct InFlightGuard {
    in_flight: Arc<DashMap<i64, ()>>,
    turn_id: i64,
}

impl Drop for InFlightGuard {
    fn drop(&mut self) {
        self.in_flight.remove(&self.turn_id);
    }
}

impl LiveJudgeShared {
    pub fn try_claim(&self, turn_id: i64) -> Option<InFlightGuard> {
        if self.in_flight.insert(turn_id, ()).is_some() {
            return None;
        }
        Some(InFlightGuard {
            in_flight: self.in_flight.clone(),
            turn_id,
        })
    }

    pub fn is_in_flight(&self, turn_id: i64) -> bool {
        self.in_flight.contains_key(&turn_id)
    }

    pub fn first_sight_of_bundle(&self, bundle_id: i64) -> bool {
        self.announced_bundle.swap(bundle_id, Ordering::AcqRel) != bundle_id
    }

    pub fn queue_reconcile_due(&self, bundle_id: i64, now_unix: i64, max_age_secs: i64) -> bool {
        self.queue_reconciled_bundle.load(Ordering::Acquire) != bundle_id
            || now_unix - self.queue_reconciled_at_unix.load(Ordering::Acquire) >= max_age_secs
    }

    pub fn note_queue_reconciled(&self, bundle_id: i64, now_unix: i64) {
        self.queue_reconciled_bundle
            .store(bundle_id, Ordering::Release);
        self.queue_reconciled_at_unix
            .store(now_unix, Ordering::Release);
    }
}

#[cfg(test)]
mod tests {
    use super::LiveJudgeShared;

    #[test]
    fn in_flight_guard_removes_on_drop() {
        let shared = LiveJudgeShared::default();
        let guard = shared.try_claim(7);
        assert!(guard.is_some());
        assert!(shared.is_in_flight(7));
        assert!(
            shared.try_claim(7).is_none(),
            "a second path must not judge the same turn"
        );
        drop(guard);
        assert!(!shared.is_in_flight(7));
        assert!(shared.try_claim(7).is_some());
    }

    #[test]
    fn queue_reconcile_is_due_at_boot_on_a_bundle_change_and_after_max_age() {
        let shared = LiveJudgeShared::default();
        assert!(shared.queue_reconcile_due(4, 1_000, 3_600));
        shared.note_queue_reconciled(4, 1_000);
        assert!(!shared.queue_reconcile_due(4, 1_100, 3_600));
        assert!(shared.queue_reconcile_due(5, 1_100, 3_600));
        assert!(shared.queue_reconcile_due(4, 4_600, 3_600));
    }

    #[test]
    fn a_bundle_is_announced_once() {
        let shared = LiveJudgeShared::default();
        assert!(shared.first_sight_of_bundle(4));
        assert!(!shared.first_sight_of_bundle(4));
        assert!(shared.first_sight_of_bundle(5));
    }
}
