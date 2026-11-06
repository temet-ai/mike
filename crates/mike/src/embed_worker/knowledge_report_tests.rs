use super::knowledge_report::{REPORT_FRESH_FOR_SECS, SweepTally};
use crate::store::persistence::project_knowledge::PENDING_BATCH;
use chrono::{DateTime, Utc};

fn at(secs: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(1_700_000_000 + secs, 0).unwrap_or_default()
}

#[test]
fn tally_empty_read_reports_a_drained_queue() {
    let mut tally = SweepTally::default();
    tally.read(3);
    tally.embedded();
    tally.embedded();
    tally.failed();
    tally.read(0);
    let report = tally.report("m", at(0));
    assert_eq!(report.pending, 0);
    assert!(!report.pending_more);
    assert!(!report.has_pending());
    assert_eq!((report.embedded, report.failed), (2, 1));
}

#[test]
fn tally_stopped_early_reports_the_unattempted_sources() {
    let mut tally = SweepTally::default();
    tally.read(5);
    tally.embedded();
    tally.stopped_with(4);
    let report = tally.report("m", at(0));
    assert_eq!(report.pending, 4);
    assert!(!report.pending_more);
    assert_eq!(report.pending_label(), "4");
}

#[test]
fn tally_full_batch_without_drain_reports_more_pending() {
    let mut tally = SweepTally::default();
    tally.read(usize::try_from(PENDING_BATCH).unwrap_or(16));
    let report = tally.report("m", at(0));
    assert_eq!(report.pending, 0);
    assert!(report.pending_more);
    assert!(report.has_pending());
    assert_eq!(report.pending_label(), "1+");
}

#[test]
fn report_freshness_requires_the_same_model_and_a_recent_pass() {
    let report = SweepTally::default().report("m", at(0));
    assert!(report.is_fresh_for("m", at(REPORT_FRESH_FOR_SECS)));
    assert!(!report.is_fresh_for("m", at(REPORT_FRESH_FOR_SECS + 1)));
    assert!(!report.is_fresh_for("other", at(1)));
}
