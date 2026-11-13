use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::{Value, json};

use super::wilson::{Bounds, wilson_bounds};
use crate::steerer::guard::Question;
use crate::steerer::guard::bars::{GateBar, gate_bar};
use crate::store::persistence::steering::promotion::GateCounts;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Precondition {
    pub name: &'static str,
    pub met: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GateEvidence {
    pub question: Question,
    pub target: String,
    pub bundle_id: Option<i64>,
    pub window_start: DateTime<Utc>,
    pub counts: GateCounts,
    pub bounds: Bounds,
    pub bar: GateBar,
    pub preconditions: Vec<Precondition>,
    pub decision_rules: Value,
    pub timing: Value,
    pub passes: bool,
    pub reasons: Vec<String>,
}

impl GateEvidence {
    pub fn to_json(&self) -> Value {
        serde_json::to_value(self).unwrap_or(Value::Null)
    }
}

pub struct GateInputs<'a> {
    pub question: Question,
    pub target: &'a str,
    pub bundle_id: Option<i64>,
    pub window_start: DateTime<Utc>,
    pub counts: GateCounts,
    pub preconditions: Vec<Precondition>,
}

fn rule_reasons(target: &str, counts: &GateCounts) -> (Value, Option<String>) {
    let rules = json!({"with_quorum": counts.with_quorum, "without_quorum": counts.would_fire - counts.with_quorum});
    let mixed = counts.with_quorum > 0 && counts.with_quorum < counts.would_fire;
    let wake_missing = target == "wake" && counts.with_quorum < counts.would_fire;
    let reason = (mixed || wake_missing).then(|| {
        format!(
            "shadow_rules_differ: {} of {} rows carry quorum votes",
            counts.with_quorum, counts.would_fire
        )
    });
    (rules, reason)
}

fn count_reasons(bar: &GateBar, counts: &GateCounts, bounds: &Bounds) -> Vec<String> {
    let mut reasons = Vec::new();
    let labelled = counts.labelled_right + counts.labelled_wrong;
    if labelled < bar.min_n {
        reasons.push(format!("labelled {labelled} < {}", bar.min_n));
    }
    if bounds.precision.is_none_or(|p| p < bar.min_precision) {
        reasons.push(format!(
            "precision {:?} < {}",
            bounds.precision, bar.min_precision
        ));
    }
    if let Some(min_low) = bar.min_wilson_low
        && bounds.low.is_none_or(|l| l < min_low)
    {
        reasons.push(format!("wilson low {:?} < {min_low}", bounds.low));
    }
    if counts.template_fires > 0 || counts.other_fires > 0 {
        reasons.push(format!(
            "{} template and {} other exempt-class would-fires",
            counts.template_fires, counts.other_fires
        ));
    }
    if counts.non_actor_fires > 0 {
        reasons.push(format!("{} non-actor would-fires", counts.non_actor_fires));
    }
    reasons
}

pub fn evaluate_gate(i: GateInputs<'_>) -> GateEvidence {
    let bar = gate_bar(i.question);
    let labelled = i.counts.labelled_right + i.counts.labelled_wrong;
    let bounds = wilson_bounds(i.counts.labelled_right, labelled);
    let mut reasons = Vec::new();
    if !bar.targets.contains(&i.target) {
        reasons.push(format!(
            "target {} is not allowed for {}",
            i.target,
            i.question.code()
        ));
    }
    reasons.extend(count_reasons(&bar, &i.counts, &bounds));
    let (decision_rules, rule_reason) = rule_reasons(i.target, &i.counts);
    reasons.extend(rule_reason);
    reasons.extend(
        i.preconditions
            .iter()
            .filter(|p| !p.met)
            .map(|p| format!("precondition {} not met", p.name)),
    );
    let share = |k: i64| (i.counts.timing_n > 0).then(|| k as f64 / i.counts.timing_n as f64);
    GateEvidence {
        question: i.question,
        target: i.target.to_owned(),
        bundle_id: i.bundle_id,
        window_start: i.window_start,
        timing: json!({"n": i.counts.timing_n, "within_grace": share(i.counts.within_grace), "within_grace_plus_60s": share(i.counts.within_grace_60)}),
        passes: reasons.is_empty(),
        counts: i.counts,
        bounds,
        bar,
        preconditions: i.preconditions,
        decision_rules,
        reasons,
    }
}

pub fn target_allowed(question: Question, target: &str) -> bool {
    gate_bar(question).targets.contains(&target)
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn counts(right: i64, wrong: i64, quorum: bool) -> GateCounts {
        let n = right + wrong;
        GateCounts {
            would_fire: n,
            labelled_right: right,
            labelled_wrong: wrong,
            with_quorum: if quorum { n } else { 0 },
            operator_fires: n,
            ..GateCounts::default()
        }
    }

    fn inputs(target: &str, c: GateCounts) -> GateInputs<'_> {
        GateInputs {
            question: Question::Relapse,
            target,
            bundle_id: Some(4),
            window_start: Utc::now(),
            counts: c,
            preconditions: vec![Precondition {
                name: "turn_judge_promoted",
                met: true,
            }],
        }
    }

    #[test]
    fn gate_34_of_40_fails_on_the_wilson_low() {
        let g = evaluate_gate(inputs("wake", counts(34, 6, true)));
        assert!(!g.passes);
        assert_eq!(g.bounds.precision, Some(0.85));
        assert!(
            g.reasons.iter().any(|r| r.starts_with("wilson low")),
            "{:?}",
            g.reasons
        );
    }

    #[test]
    fn gate_36_of_40_passes_for_wake_with_quorum() {
        let g = evaluate_gate(inputs("wake", counts(36, 4, true)));
        assert!(g.passes, "{:?}", g.reasons);
        let no_votes = evaluate_gate(inputs("wake", counts(36, 4, false)));
        assert!(
            no_votes
                .reasons
                .iter()
                .any(|r| r.starts_with("shadow_rules_differ"))
        );
    }

    #[test]
    fn gate_refuses_wake_for_handback() {
        assert!(!target_allowed(Question::AuthorisedHandback, "wake"));
        assert!(!target_allowed(Question::UnobservedDone, "wake"));
        assert!(target_allowed(Question::UnbackedWait, "wake"));
    }
}
