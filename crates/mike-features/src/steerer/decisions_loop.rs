use chrono::{DateTime, Duration, Utc};
use serde::Serialize;
use serde_json::{Value, json};

use crate::cross_session::threshold::wilson_lower_bound;
use crate::llm::decisions::loop_store::{LabelledDecision, PointCounts};
use crate::steerer::precheck::score::auroc;
use crate::store::persistence::launch_repo::LaunchPersist as _;
use crate::store::persistence::work_threads::entry::EntryInput;
use crate::store::persistence::{PersistError, PersistHandle};

pub const THREAD: &str = "jev-decisions";
pub const OVERDUE: Duration = Duration::hours(24);
pub const WINDOW: Duration = Duration::days(14);
pub const MAX_FAIL_OPEN: f64 = 0.05;
pub const HOLD_PRECISION_BAR: f64 = 0.8;
pub const MIN_LABELS: i64 = 30;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PointScore {
    pub counts: PointCounts,
    pub auroc: Option<f64>,
    pub held_labelled: usize,
    pub held_wrong: usize,
    pub held_precision_lower: Option<f64>,
    pub sent_labelled: usize,
    pub sent_right: usize,
    pub labels_per_day: f64,
    pub days_to_min_labels: Option<f64>,
    pub applied_labelled: usize,
    pub applied_right: usize,
    pub applied_precision_lower: Option<f64>,
    pub findings: Vec<String>,
}

pub const APPLIED_PRECISION_BAR: f64 = 0.6;

fn applied_precision(point: &str, rows: &[&LabelledDecision]) -> (usize, usize, Option<f64>) {
    if point == crate::steerer::precheck::POINT {
        return (0, 0, None);
    }
    let applied: Vec<&&LabelledDecision> = rows
        .iter()
        .filter(|l| l.applied.as_deref().is_some_and(|a| a != "none"))
        .collect();
    let right = applied.iter().filter(|l| is_right(&l.label)).count();
    let lower = wilson_lower_bound(right as f64, applied.len() as f64);
    (applied.len(), right, lower)
}

fn applied_finding(n: usize, lower: Option<f64>) -> Option<String> {
    let lower = lower.filter(|_| n >= 10)?;
    (lower < APPLIED_PRECISION_BAR).then(|| {
        format!(
            "of {n} labelled applied decisions, the share that were right has a lower bound of {lower:.2}, under the bar {APPLIED_PRECISION_BAR:.2}"
        )
    })
}

fn is_right(label: &str) -> bool {
    label == "right"
}

fn findings(counts: &PointCounts, held_lower: Option<f64>, held_n: usize) -> Vec<String> {
    let mut out = Vec::new();
    let calls = counts.calls.max(1) as f64;
    let fail_open = counts.fail_open as f64 / calls;
    if fail_open > MAX_FAIL_OPEN {
        out.push(format!(
            "fails open on {:.0}% of {} calls (bar {:.0}%)",
            fail_open * 100.0,
            counts.calls,
            MAX_FAIL_OPEN * 100.0
        ));
    }
    if counts.schema_errors > 0 {
        out.push(format!("{} schema errors", counts.schema_errors));
    }
    if let Some(lower) = held_lower.filter(|_| held_n >= 10)
        && lower < HOLD_PRECISION_BAR
    {
        out.push(format!(
            "of {held_n} labelled holds, the share that held a wrong nudge has a lower bound of {lower:.2}, under the bar {HOLD_PRECISION_BAR:.2}"
        ));
    }
    if counts.calls > 0 && counts.applied_on == 0 {
        out.push("no call in mode on applied anything".to_owned());
    }
    out
}

pub fn score_point(counts: PointCounts, labelled: &[LabelledDecision]) -> PointScore {
    let rows: Vec<&LabelledDecision> = labelled
        .iter()
        .filter(|l| l.point == counts.point)
        .collect();
    let scored: Vec<(f64, bool)> = rows
        .iter()
        .filter_map(|l| l.score.map(|s| (s, is_right(&l.label))))
        .collect();
    let held: Vec<&&LabelledDecision> = rows
        .iter()
        .filter(|l| l.applied.as_deref() == Some("held"))
        .collect();
    let sent: Vec<&&LabelledDecision> = rows
        .iter()
        .filter(|l| l.applied.as_deref() == Some("sent"))
        .collect();
    let held_wrong = held.iter().filter(|l| !is_right(&l.label)).count();
    let held_lower = wilson_lower_bound(held_wrong as f64, held.len() as f64);
    let per_day = counts.labelled_last_7d as f64 / 7.0;
    let missing = (MIN_LABELS - counts.labelled).max(0) as f64;
    let (applied_n, applied_right, applied_lower) = applied_precision(&counts.point, &rows);
    let mut found = findings(&counts, held_lower, held.len());
    found.extend(applied_finding(applied_n, applied_lower));
    PointScore {
        applied_labelled: applied_n,
        applied_right,
        applied_precision_lower: applied_lower,
        auroc: auroc(&scored),
        held_labelled: held.len(),
        held_wrong,
        held_precision_lower: held_lower,
        sent_labelled: sent.len(),
        sent_right: sent.iter().filter(|l| is_right(&l.label)).count(),
        labels_per_day: per_day,
        days_to_min_labels: (per_day > 0.0).then(|| missing / per_day),
        findings: found,
        counts,
    }
}

pub async fn overdue(persist: &PersistHandle, now: DateTime<Utc>) -> Result<bool, PersistError> {
    Ok(crate::llm::decisions::store(persist)
        .last_decision_loop_at()
        .await?
        .is_none_or(|last| now - last >= OVERDUE))
}

async fn file_findings(persist: &PersistHandle, scores: &[PointScore]) -> Option<i32> {
    let failing: Vec<&PointScore> = scores.iter().filter(|s| !s.findings.is_empty()).collect();
    if failing.is_empty() {
        return None;
    }
    let body = failing
        .iter()
        .map(|s| format!("{}: {}", s.counts.point, s.findings.join("; ")))
        .collect::<Vec<_>>()
        .join("\n");
    let project = sqlx::query_scalar::<_, i64>(
        "SELECT project_id FROM work.threads WHERE slug = $1 ORDER BY id LIMIT 1",
    )
    .bind(THREAD)
    .fetch_optional(persist.pool())
    .await
    .ok()??;
    let data = json!({ "scores": failing });
    let input = EntryInput {
        slug: THREAD,
        kind: "finding",
        body: &format!("decisions_loop found decision points that need fixing:\n{body}"),
        summary: Some("decisions_loop: decision points below their bar"),
        session_uuid: None,
        commit_sha: None,
        data: Some(&data),
    };
    match persist
        .launch()
        .add_work_thread_entry(project, &input)
        .await
    {
        Ok(outcome) => Some(outcome.entry.ord),
        Err(error) => {
            tracing::warn!(%error, "decisions_loop: could not file the finding on the thread");
            None
        }
    }
}

pub async fn run(persist: &PersistHandle, now: DateTime<Utc>) -> Result<Value, PersistError> {
    let store = crate::llm::decisions::store(persist);
    let synced = store
        .sync_evaluation_outcomes(crate::steerer::precheck::POINT)
        .await?;
    let since = now - WINDOW;
    let counts = store.decision_point_counts(since).await?;
    let labelled = store.labelled_decisions(since).await?;
    let scores: Vec<PointScore> = counts
        .into_iter()
        .map(|c| score_point(c, &labelled))
        .collect();
    let finding_entry = file_findings(persist, &scores).await;
    let report = json!({
        "synced_outcomes": synced,
        "window_days": WINDOW.num_days(),
        "points": scores,
        "finding_entry": finding_entry,
    });
    let id = store.insert_decision_loop_run(now, &report).await?;
    tracing::info!(
        run = id,
        points = scores.len(),
        failing = scores.iter().filter(|s| !s.findings.is_empty()).count(),
        synced_outcomes = synced,
        "decisions_loop: scored the decision points"
    );
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn counts(point: &str, calls: i64, applied_on: i64, fail_open: i64) -> PointCounts {
        PointCounts {
            point: point.to_owned(),
            calls,
            applied_on,
            fail_open,
            labelled: 10,
            labelled_last_7d: 7,
            ..PointCounts::default()
        }
    }

    fn label(score: f64, applied: &str, label: &str) -> LabelledDecision {
        LabelledDecision {
            point: "p".to_owned(),
            score: Some(score),
            applied: Some(applied.to_owned()),
            label: label.to_owned(),
        }
    }

    #[test]
    fn a_point_that_fails_open_too_often_is_a_finding() {
        let s = score_point(counts("p", 100, 90, 10), &[]);
        assert!(
            s.findings
                .iter()
                .any(|f| f.starts_with("fails open on 10%"))
        );
    }

    #[test]
    fn a_healthy_point_has_no_findings_and_counts_labels_per_day() {
        let s = score_point(counts("p", 100, 90, 1), &[]);
        assert!(s.findings.is_empty(), "{:?}", s.findings);
        assert!((s.labels_per_day - 1.0).abs() < 1e-9);
        assert_eq!(s.days_to_min_labels, Some(20.0));
    }

    #[test]
    fn holds_that_withhold_right_nudges_are_a_finding() {
        let labelled: Vec<_> = (0..12)
            .map(|i| label(0.1, "held", if i < 6 { "right" } else { "wrong" }))
            .collect();
        let s = score_point(counts("p", 100, 90, 0), &labelled);
        assert_eq!(s.held_labelled, 12);
        assert!(s.findings.iter().any(|f| f.contains("labelled holds")));
    }

    #[test]
    fn auroc_is_computed_from_the_composed_score() {
        let labelled = vec![label(0.9, "sent", "right"), label(0.1, "held", "wrong")];
        let s = score_point(counts("p", 2, 2, 0), &labelled);
        assert_eq!(s.auroc, Some(1.0));
        assert_eq!(s.sent_right, 1);
    }

    #[test]
    fn a_point_whose_applied_decisions_are_mostly_wrong_is_a_finding() {
        let labelled: Vec<_> = (0..12)
            .map(|i| label(0.8, "chosen", if i < 3 { "right" } else { "wrong" }))
            .collect();
        let s = score_point(counts("p", 100, 90, 0), &labelled);
        assert_eq!(s.applied_labelled, 12);
        assert_eq!(s.applied_right, 3);
        assert!(s.findings.iter().any(|f| f.contains("labelled applied")));
    }

    #[test]
    fn calls_that_never_apply_in_mode_on_are_a_finding() {
        let s = score_point(counts("p", 5, 0, 0), &[]);
        assert!(s.findings.iter().any(|f| f.contains("applied anything")));
    }
}
