use std::collections::{BTreeMap, BTreeSet, HashMap};

use serde::Serialize;

use super::bootstrap::{clustered_rd_ci, risk_difference_ci, seed_for};
use super::outcome::{Outcome, scored_outcome};
use crate::store::persistence::steering::promotion::ArmRow;

pub const READ_POINT_HOLDBACK: usize = 34;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ArmSummary {
    pub n: usize,
    pub bad: usize,
    pub p_bad: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Estimate {
    pub treated: ArmSummary,
    pub holdback: ArmSummary,
    pub rd: Option<f64>,
    pub ci: Option<(f64, f64)>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EffectEstimate {
    pub question: Option<String>,
    pub as_of_secs: i64,
    pub primary: Estimate,
    pub secondary: Estimate,
    pub per_protocol_not_randomised: Estimate,
    pub missing: BTreeMap<String, usize>,
    pub events: usize,
    pub status: &'static str,
}

fn summary(outcomes: &[bool]) -> ArmSummary {
    let bad = outcomes.iter().filter(|b| **b).count();
    ArmSummary {
        n: outcomes.len(),
        bad,
        p_bad: (!outcomes.is_empty()).then(|| bad as f64 / outcomes.len() as f64),
    }
}

fn estimate(holdback: &[bool], treated: &[bool], seed: u64) -> Estimate {
    let h = summary(holdback);
    let t = summary(treated);
    Estimate {
        rd: h.p_bad.zip(t.p_bad).map(|(a, b)| a - b),
        ci: risk_difference_ci(holdback, treated, seed),
        holdback: h,
        treated: t,
    }
}

type Scored<'a> = (&'a ArmRow, bool);

fn split(rows: &[Scored<'_>]) -> (Vec<bool>, Vec<bool>) {
    let holdback = rows
        .iter()
        .filter(|(r, _)| r.arm.as_deref() == Some("holdback"))
        .map(|(_, b)| *b)
        .collect();
    let treated = rows
        .iter()
        .filter(|(r, _)| r.arm.as_deref() == Some("treated"))
        .map(|(_, b)| *b)
        .collect();
    (holdback, treated)
}

fn first_per_session<'a>(rows: &[Scored<'a>]) -> Vec<Scored<'a>> {
    let mut seen: BTreeSet<(i64, Option<String>)> = BTreeSet::new();
    rows.iter()
        .filter(|(r, _)| seen.insert((r.session_id, r.question.clone())))
        .copied()
        .collect()
}

fn clusters(rows: &[Scored<'_>], arm: &str) -> Vec<Vec<bool>> {
    let mut by_session: BTreeMap<i64, Vec<bool>> = BTreeMap::new();
    for (r, b) in rows.iter().filter(|(r, _)| r.arm.as_deref() == Some(arm)) {
        by_session.entry(r.session_id).or_default().push(*b);
    }
    by_session.into_values().collect()
}

pub fn estimate_effect(
    rows: &[ArmRow],
    end_labels: &HashMap<i64, String>,
    question: Option<&str>,
    as_of_secs: i64,
) -> EffectEstimate {
    let seed = seed_for(question.unwrap_or("pooled"), as_of_secs);
    let mut missing: BTreeMap<String, usize> = BTreeMap::new();
    let mut scored: Vec<Scored<'_>> = Vec::new();
    for r in rows {
        let end = r
            .scored_turn_id
            .and_then(|t| end_labels.get(&t))
            .map(String::as_str);
        match scored_outcome(r, end) {
            Outcome::Bad => scored.push((r, true)),
            Outcome::Good => scored.push((r, false)),
            Outcome::Missing(reason) => *missing.entry(reason.to_owned()).or_default() += 1,
        }
    }
    let first = first_per_session(&scored);
    let (ph, pt) = split(&first);
    let (sh, st) = split(&scored);
    let secondary_ci = clustered_rd_ci(
        &clusters(&scored, "holdback"),
        &clusters(&scored, "treated"),
        seed ^ 1,
    );
    let mut secondary = estimate(&sh, &st, seed ^ 1);
    secondary.ci = secondary_ci;
    let delivered: Vec<Scored<'_>> = scored
        .iter()
        .copied()
        .filter(|(r, _)| r.arm.as_deref() == Some("holdback") || r.delivered)
        .collect();
    let (dh, dt) = split(&delivered);
    let primary = estimate(&ph, &pt, seed);
    let status = if primary.holdback.n >= READ_POINT_HOLDBACK {
        "readable"
    } else {
        "monitoring"
    };
    EffectEstimate {
        question: question.map(str::to_owned),
        as_of_secs,
        primary,
        secondary,
        per_protocol_not_randomised: estimate(&dh, &dt, seed ^ 2),
        missing,
        events: rows.len(),
        status,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i64, session: i64, arm: &str, reaction: &str) -> ArmRow {
        ArmRow {
            id,
            uuid: uuid::Uuid::nil(),
            session_id: session,
            operator_turn_id: id,
            question: Some("relapse".to_owned()),
            arm: Some(arm.to_owned()),
            holdback_draw: Some(0.5),
            arm_anchor_turn_id: Some(id),
            due_at: None,
            reach_at_due: None,
            dispatched_at: chrono::Utc::now(),
            outcome_label: None,
            label_source: None,
            carrier_request_id: Some(id),
            scored_turn_id: Some(id + 1),
            scored_reaction: Some(reaction.to_owned()),
            scored_is_last: false,
            delivered: true,
            wake_state: None,
        }
    }

    #[test]
    fn estimate_effect_primary_uses_first_event_per_session() {
        let rows = vec![
            row(1, 1, "holdback", "corrected"),
            row(2, 1, "holdback", "accepted"),
            row(3, 2, "treated", "accepted"),
            row(4, 3, "treated", "corrected"),
        ];
        let e = estimate_effect(&rows, &HashMap::new(), Some("relapse"), 0);
        assert_eq!(e.primary.holdback.n, 1);
        assert_eq!(e.primary.holdback.bad, 1);
        assert_eq!(e.primary.treated.n, 2);
        assert_eq!(e.primary.rd, Some(0.5));
        assert_eq!(e.secondary.holdback.n, 2);
        assert_eq!(e.status, "monitoring");
    }
}
