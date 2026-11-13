use serde::Serialize;
use serde_json::{Value, json};

use super::verifier_baseline::{FLOOR_PRECISION, FLOOR_RECALL, baseline_json};
use super::wilson::wilson_bounds;
use crate::store::persistence::steering::promotion::VerifierPairs;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VerifierDecision {
    pub passes: bool,
    pub reasons: Vec<String>,
    pub evidence: Value,
}

pub struct ReplacementInputs<'a> {
    pub gate_passed: bool,
    pub pairs: &'a VerifierPairs,
    pub overrides_accounted: bool,
    pub exempt_fires: i64,
}

fn ratio(k: i64, n: i64) -> Option<f64> {
    (n > 0).then(|| k as f64 / n as f64)
}

pub fn decide_replacement(i: &ReplacementInputs<'_>) -> VerifierDecision {
    let p = i.pairs;
    let guard_fired = p.both_fired + p.guard_only;
    let guard_precision = ratio(p.guard_corrected, guard_fired);
    let verifier_precision = ratio(p.verifier_corrected, p.verifier_counted);
    let guard_recall = ratio(p.guard_hits, p.relevant);
    let verifier_recall = ratio(p.verifier_hits, p.relevant);
    let mut reasons = Vec::new();
    if !i.gate_passed {
        reasons.push("unobserved_done's own gate fails".to_owned());
    }
    let beats = |a: Option<f64>, b: Option<f64>| a.is_some_and(|a| b.is_none_or(|b| a >= b));
    if !beats(guard_precision, verifier_precision)
        || guard_precision.is_none_or(|g| g < FLOOR_PRECISION)
    {
        reasons.push(format!("precision {guard_precision:?} against verifier {verifier_precision:?} and floor {FLOOR_PRECISION}"));
    }
    if !beats(guard_recall, verifier_recall) || guard_recall.is_none_or(|g| g < FLOOR_RECALL) {
        reasons.push(format!(
            "recall {guard_recall:?} against verifier {verifier_recall:?} and floor {FLOOR_RECALL}"
        ));
    }
    if !i.overrides_accounted {
        reasons.push("the verifier's deterministic overrides are not accounted for".to_owned());
    }
    if i.exempt_fires > 0 {
        reasons.push(format!(
            "{} would-fires on template or exempt sessions",
            i.exempt_fires
        ));
    }
    if p.guard_usd > p.verifier_usd && p.verifier_calls > 0 {
        reasons.push(format!(
            "guard cost {:.4} exceeds verifier cost {:.4}",
            p.guard_usd, p.verifier_usd
        ));
    }
    VerifierDecision {
        passes: reasons.is_empty(),
        reasons,
        evidence: json!({
            "pairs": p,
            "guard": {"precision": wilson_bounds(p.guard_corrected, guard_fired), "recall": guard_recall},
            "verifier": {"precision": wilson_bounds(p.verifier_corrected, p.verifier_counted), "recall": verifier_recall},
            "baseline": baseline_json(),
        }),
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    fn pairs(guard_corrected: i64, verifier_corrected: i64) -> VerifierPairs {
        VerifierPairs {
            turns: 100,
            both_fired: 10,
            guard_only: 10,
            verifier_only: 10,
            verifier_counted: 20,
            verifier_corrected,
            guard_corrected,
            relevant: 20,
            verifier_hits: 3,
            guard_hits: 6,
            ..VerifierPairs::default()
        }
    }

    #[test]
    fn replacement_needs_q4_to_beat_the_verifier_and_the_floor() {
        let good = decide_replacement(&ReplacementInputs {
            gate_passed: true,
            pairs: &pairs(14, 10),
            overrides_accounted: true,
            exempt_fires: 0,
        });
        assert!(good.passes, "{:?}", good.reasons);
        let worse = decide_replacement(&ReplacementInputs {
            gate_passed: true,
            pairs: &pairs(8, 12),
            overrides_accounted: true,
            exempt_fires: 0,
        });
        assert!(!worse.passes);
        let unaccounted = decide_replacement(&ReplacementInputs {
            gate_passed: true,
            pairs: &pairs(14, 10),
            overrides_accounted: false,
            exempt_fires: 0,
        });
        assert!(!unaccounted.passes);
    }
}
