use super::*;
use crate::embed_worker::knowledge_report::KnowledgeSweepReport;

fn row() -> CapabilityRow {
    CapabilityRow {
        id: "project_knowledge",
        label: "Project knowledge embeddings",
        state: CapabilityState::Inert,
        detail: String::new(),
        fix: "check".into(),
    }
}

fn report(pending: usize, pending_more: bool) -> KnowledgeSweepReport {
    KnowledgeSweepReport {
        model: "m".into(),
        at: chrono::Utc::now(),
        pending,
        pending_more,
        embedded: 0,
        failed: 0,
    }
}

fn counts(ready: i64, failed: i64) -> KnowledgeStateCounts {
    KnowledgeStateCounts { ready, failed }
}

#[test]
fn coverage_drained_sweep_with_ready_sources_is_active_and_clears_the_fix() {
    let mut row = row();
    apply_coverage(&mut row, counts(12, 0), Some(&report(0, false)));
    assert_eq!(row.state, CapabilityState::Active);
    assert!(row.fix.is_empty());
    assert!(
        row.detail
            .starts_with("12 sources embedded, 0 pending, 0 with recorded failures")
    );
}

#[test]
fn coverage_pending_from_the_sweep_degrades() {
    let mut row = row();
    apply_coverage(&mut row, counts(12, 0), Some(&report(3, false)));
    assert_eq!(row.state, CapabilityState::Degraded);
    assert!(row.detail.contains("3 pending"));
    assert_eq!(row.fix, "check");
}

#[test]
fn coverage_full_batch_reports_more_pending() {
    let mut row = row();
    apply_coverage(&mut row, counts(12, 0), Some(&report(0, true)));
    assert_eq!(row.state, CapabilityState::Degraded);
    assert!(row.detail.contains("1+ pending"));
}

#[test]
fn coverage_recorded_failures_degrade_without_a_sweep_report() {
    let mut row = row();
    apply_coverage(&mut row, counts(5, 2), None);
    assert_eq!(row.state, CapabilityState::Degraded);
    assert!(row.detail.contains("2 with recorded failures"));
    assert!(row.detail.contains("has not reported on this machine"));
}

#[test]
fn coverage_without_a_sweep_report_stays_active_on_ready_sources() {
    let mut row = row();
    apply_coverage(&mut row, counts(5, 0), None);
    assert_eq!(row.state, CapabilityState::Active);
}

#[test]
fn coverage_nothing_embedded_is_inert() {
    let mut row = row();
    apply_coverage(&mut row, counts(0, 0), Some(&report(0, false)));
    assert_eq!(row.state, CapabilityState::Inert);
    assert_eq!(row.fix, "check");
}

#[test]
fn capped_label_marks_counts_past_the_cap() {
    assert_eq!(
        capped_label(KNOWLEDGE_COUNT_CAP),
        KNOWLEDGE_COUNT_CAP.to_string()
    );
    assert_eq!(
        capped_label(KNOWLEDGE_COUNT_CAP + 1),
        format!("{KNOWLEDGE_COUNT_CAP}+")
    );
    assert_eq!(capped_label(0), "0");
}
