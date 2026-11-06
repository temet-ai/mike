use crate::store::persistence::project_knowledge::PENDING_BATCH;
use arc_swap::ArcSwapOption;
use chrono::{DateTime, Utc};
use std::sync::Arc;

pub(crate) const REPORT_FRESH_FOR_SECS: i64 = 150;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KnowledgeSweepReport {
    pub model: String,
    pub at: DateTime<Utc>,
    pub pending: usize,
    pub pending_more: bool,
    pub embedded: usize,
    pub failed: usize,
}

impl KnowledgeSweepReport {
    pub(crate) fn has_pending(&self) -> bool {
        self.pending > 0 || self.pending_more
    }

    pub(crate) fn pending_label(&self) -> String {
        if self.pending_more {
            format!("{}+", self.pending.max(1))
        } else {
            self.pending.to_string()
        }
    }

    pub(crate) fn is_fresh_for(&self, model: &str, now: DateTime<Utc>) -> bool {
        self.model == model && (now - self.at).num_seconds() <= REPORT_FRESH_FOR_SECS
    }
}

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub(crate) struct SweepTally {
    last_read: usize,
    unattempted: usize,
    drained: bool,
    embedded: usize,
    failed: usize,
}

impl SweepTally {
    pub(crate) fn read(&mut self, found: usize) {
        self.last_read = found;
        self.unattempted = 0;
        self.drained = found == 0;
    }

    pub(crate) fn stopped_with(&mut self, unattempted: usize) {
        self.unattempted = unattempted;
    }

    pub(crate) fn embedded(&mut self) {
        self.embedded += 1;
    }

    pub(crate) fn failed(&mut self) {
        self.failed += 1;
    }

    pub(crate) fn report(self, model: &str, at: DateTime<Utc>) -> KnowledgeSweepReport {
        let full_batch = i64::try_from(self.last_read).unwrap_or(i64::MAX) >= PENDING_BATCH;
        KnowledgeSweepReport {
            model: model.to_owned(),
            at,
            pending: if self.drained { 0 } else { self.unattempted },
            pending_more: !self.drained && full_batch,
            embedded: self.embedded,
            failed: self.failed,
        }
    }
}

static LAST_SWEEP: ArcSwapOption<KnowledgeSweepReport> = ArcSwapOption::const_empty();

pub(crate) fn publish(report: KnowledgeSweepReport) {
    LAST_SWEEP.store(Some(Arc::new(report)));
}

pub(crate) fn fresh_report(model: &str, now: DateTime<Utc>) -> Option<Arc<KnowledgeSweepReport>> {
    LAST_SWEEP
        .load_full()
        .filter(|report| report.is_fresh_for(model, now))
}
