use std::collections::{BTreeMap, BTreeSet};

use serde::Serialize;

use super::Question;
use super::bars::promotion_bar;
use crate::eval::score::bootstrap::bootstrap_ci;
use crate::store::persistence::steering::guard::GuardMetricRow;

const WILSON_Z: f64 = 1.96;
const METRIC_SEED: u64 = 16;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Interval {
    pub k: f64,
    pub n: f64,
    pub value: Option<f64>,
    pub low: Option<f64>,
    pub high: Option<f64>,
}

pub fn wilson(k: f64, n: f64) -> Interval {
    if n <= 0.0 {
        return Interval {
            k,
            n,
            value: None,
            low: None,
            high: None,
        };
    }
    let p = k / n;
    let z2 = WILSON_Z * WILSON_Z;
    let centre = (p + z2 / (2.0 * n)) / (1.0 + z2 / n);
    let half = WILSON_Z * ((p * (1.0 - p) / n) + z2 / (4.0 * n * n)).sqrt() / (1.0 + z2 / n);
    Interval {
        k,
        n,
        value: Some(p),
        low: Some((centre - half).max(0.0)),
        high: Some((centre + half).min(1.0)),
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct Funnel {
    pub rows: usize,
    pub exempt: BTreeMap<String, usize>,
    pub gate_closed: BTreeMap<String, usize>,
    pub gate_open_g2: usize,
    pub gate_open_g2b: usize,
    pub gate_open_both: usize,
    pub gate_open: usize,
    pub called: usize,
    pub daily_cap: usize,
    pub would_fire: usize,
    pub labelled: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct QuestionMetrics {
    pub question: String,
    pub trigger_point: String,
    pub funnel: Funnel,
    pub precision: Interval,
    pub recall: Option<f64>,
    pub recall_ci: Option<(f64, f64)>,
    pub first_vote_precision: Interval,
    pub reach_share: Interval,
    pub bar: f64,
    pub daily_cap_excluded: usize,
}

fn fired(r: &GuardMetricRow) -> bool {
    r.status == "resolved_correction"
}

fn is_label(r: &GuardMetricRow, label: &str) -> bool {
    r.outcome_label.as_deref() == Some(label)
}

pub fn miss_weight(r: &GuardMetricRow, recall_sample_pct: i64) -> f64 {
    if r.gate_open {
        1.0
    } else if r.recall_sample && recall_sample_pct > 0 {
        100.0 / recall_sample_pct as f64
    } else {
        0.0
    }
}

pub fn recall_of(rows: &[&GuardMetricRow], pct: i64) -> Option<f64> {
    let right = rows
        .iter()
        .filter(|r| fired(r) && is_label(r, "right"))
        .count() as f64;
    let misses: f64 = rows
        .iter()
        .filter(|r| {
            !fired(r) && is_label(r, "wrong") && r.abstention.as_deref() != Some("daily_cap")
        })
        .map(|r| miss_weight(r, pct))
        .sum();
    (right + misses > 0.0).then(|| right / (right + misses))
}

fn precision_of(rows: &[&GuardMetricRow], fires: impl Fn(&GuardMetricRow) -> bool) -> Interval {
    let right = rows
        .iter()
        .filter(|r| fires(r) && is_label(r, "right"))
        .count() as f64;
    let wrong = rows
        .iter()
        .filter(|r| fires(r) && is_label(r, "wrong"))
        .count() as f64;
    wilson(right, right + wrong)
}

fn funnel(rows: &[&GuardMetricRow]) -> Funnel {
    let mut f = Funnel {
        rows: rows.len(),
        ..Funnel::default()
    };
    for r in rows {
        if let Some(e) = &r.exemption {
            *f.exempt.entry(e.clone()).or_default() += 1;
            continue;
        }
        if !r.gate_open {
            *f.gate_closed
                .entry(r.gate_reason.clone().unwrap_or_default())
                .or_default() += 1;
            continue;
        }
        f.gate_open += 1;
        match (r.arm.as_deref(), r.other_g2_open || r.other_g2b_open) {
            (Some("g2"), true) | (Some("g2b"), true) => f.gate_open_both += 1,
            (Some("g2"), false) => f.gate_open_g2 += 1,
            (Some("g2b"), false) => f.gate_open_g2b += 1,
            _ => {}
        }
        if r.abstention.as_deref() == Some("daily_cap") {
            f.daily_cap += 1;
        } else {
            f.called += 1;
        }
        f.would_fire += usize::from(fired(r));
    }
    f.labelled = rows.iter().filter(|r| r.outcome_label.is_some()).count();
    f
}

fn group_metrics(
    question: &str,
    trigger: &str,
    rows: &[&GuardMetricRow],
    pct: i64,
) -> QuestionMetrics {
    let fires: Vec<&GuardMetricRow> = rows.iter().copied().filter(|r| fired(r)).collect();
    let reachable = fires
        .iter()
        .filter(|r| r.stream_open || r.channel_live)
        .count() as f64;
    let owned: Vec<GuardMetricRow> = rows.iter().map(|r| (*r).clone()).collect();
    let recall_ci = bootstrap_ci(&owned, METRIC_SEED, |sample| recall_of(sample, pct));
    QuestionMetrics {
        question: question.to_owned(),
        trigger_point: trigger.to_owned(),
        funnel: funnel(rows),
        precision: precision_of(rows, fired),
        recall: recall_of(rows, pct),
        recall_ci,
        first_vote_precision: precision_of(rows, |r| r.first_vote_fires == Some(true)),
        reach_share: wilson(reachable, fires.len() as f64),
        bar: Question::parse(question).map_or(0.0, promotion_bar),
        daily_cap_excluded: rows
            .iter()
            .filter(|r| r.abstention.as_deref() == Some("daily_cap"))
            .count(),
    }
}

pub fn question_metrics(rows: &[GuardMetricRow], recall_sample_pct: i64) -> Vec<QuestionMetrics> {
    let mut groups: BTreeMap<(String, String), Vec<&GuardMetricRow>> = BTreeMap::new();
    for r in rows.iter().filter(|r| !r.superseded) {
        let key = (
            r.question.clone().unwrap_or_default(),
            r.trigger_point.clone().unwrap_or_default(),
        );
        groups.entry(key).or_default().push(r);
    }
    groups
        .iter()
        .map(|((q, t), rs)| group_metrics(q, t, rs, recall_sample_pct))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct AuditReport {
    pub audit_set_sessions: usize,
    pub audit_set_fires: usize,
    pub offending_evaluations: Vec<uuid::Uuid>,
}

pub fn audit(rows: &[GuardMetricRow], audit_sessions: &[i64]) -> AuditReport {
    let set: BTreeSet<i64> = audit_sessions.iter().copied().collect();
    let offending: Vec<uuid::Uuid> = rows
        .iter()
        .filter(|r| fired(r) && set.contains(&r.session_id))
        .map(|r| r.uuid)
        .collect();
    AuditReport {
        audit_set_sessions: set.len(),
        audit_set_fires: offending.len(),
        offending_evaluations: offending,
    }
}
