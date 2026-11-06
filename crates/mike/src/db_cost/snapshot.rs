use std::collections::{HashMap, VecDeque};

use chrono::{DateTime, Duration, Utc};

const MAX_SPAN_SECS: i64 = 7 * 86_400;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub(crate) struct Counters {
    pub(crate) calls: i64,
    pub(crate) generic_plan_calls: i64,
    pub(crate) custom_plan_calls: i64,
    pub(crate) exec_ms: f64,
    pub(crate) blks_hit: i64,
    pub(crate) blks_read: i64,
    pub(crate) blks_dirtied: i64,
    pub(crate) blks_written: i64,
}

impl Counters {
    pub(crate) fn since(self, earlier: Option<Counters>) -> Counters {
        match earlier {
            Some(base) if base.calls <= self.calls => Counters {
                calls: self.calls - base.calls,
                generic_plan_calls: (self.generic_plan_calls - base.generic_plan_calls).max(0),
                custom_plan_calls: (self.custom_plan_calls - base.custom_plan_calls).max(0),
                exec_ms: (self.exec_ms - base.exec_ms).max(0.0),
                blks_hit: (self.blks_hit - base.blks_hit).max(0),
                blks_read: (self.blks_read - base.blks_read).max(0),
                blks_dirtied: (self.blks_dirtied - base.blks_dirtied).max(0),
                blks_written: (self.blks_written - base.blks_written).max(0),
            },
            _ => self,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Snapshot {
    pub(crate) taken_at: DateTime<Utc>,
    pub(crate) stats_reset: Option<DateTime<Utc>>,
    pub(crate) statements: HashMap<i64, Counters>,
}

impl Snapshot {
    fn zero_at_reset(reset: DateTime<Utc>, after: &Snapshot) -> Snapshot {
        Snapshot {
            taken_at: reset.min(after.taken_at),
            stats_reset: after.stats_reset,
            statements: HashMap::new(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) struct Window {
    pub(crate) from: DateTime<Utc>,
    pub(crate) to: DateTime<Utc>,
    pub(crate) stats_reset: Option<DateTime<Utc>>,
    pub(crate) deltas: Vec<(i64, Counters)>,
}

impl Window {
    pub(crate) fn secs(&self) -> f64 {
        let millis = (self.to - self.from).num_milliseconds();
        (millis as f64 / 1000.0).max(1.0)
    }
}

pub(crate) struct SnapshotRing {
    span: Duration,
    snapshots: VecDeque<Snapshot>,
}

impl SnapshotRing {
    pub(crate) fn new(span_secs: u64) -> Self {
        let span = i64::try_from(span_secs)
            .unwrap_or(MAX_SPAN_SECS)
            .clamp(1, MAX_SPAN_SECS);
        Self {
            span: Duration::seconds(span),
            snapshots: VecDeque::new(),
        }
    }

    pub(crate) fn push(&mut self, snapshot: Snapshot) {
        let reset_since_last = self
            .snapshots
            .back()
            .is_some_and(|newest| newest.stats_reset != snapshot.stats_reset);
        if reset_since_last {
            self.snapshots.clear();
            if let Some(reset) = snapshot.stats_reset {
                self.snapshots
                    .push_back(Snapshot::zero_at_reset(reset, &snapshot));
            }
        }
        let cutoff = snapshot.taken_at - self.span;
        self.snapshots.push_back(snapshot);
        while self.snapshots.len() > 2
            && self.snapshots.get(1).is_some_and(|s| s.taken_at <= cutoff)
        {
            self.snapshots.pop_front();
        }
    }

    pub(crate) fn window(&self) -> Option<Window> {
        if self.snapshots.len() < 2 {
            return None;
        }
        let first = self.snapshots.front()?;
        let last = self.snapshots.back()?;
        let mut deltas: Vec<(i64, Counters)> = last
            .statements
            .iter()
            .map(|(id, now)| (*id, now.since(first.statements.get(id).copied())))
            .filter(|(_, delta)| delta.calls > 0)
            .collect();
        deltas.sort_by_key(|(id, _)| *id);
        Some(Window {
            from: first.taken_at,
            to: last.taken_at,
            stats_reset: last.stats_reset,
            deltas,
        })
    }
}

#[cfg(test)]
#[path = "snapshot_tests.rs"]
mod snapshot_tests;
