use serde::Serialize;
use serde_json::{Value, json};

use super::sql_label_mix::{MIX_CLASSES, MixWindowRow};
use super::types::AlarmThresholds;

const BUNDLE_SHARE_FLOOR: f64 = 0.10;
const BASELINE_MULTIPLE: i64 = 3;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShiftStats {
    pub n_current: i64,
    pub n_baseline: i64,
    pub tvd: f64,
    pub chi2: f64,
    pub over_thresholds: bool,
    pub explained_by: Vec<String>,
}

impl ShiftStats {
    pub fn fires(&self) -> bool {
        self.over_thresholds && self.explained_by.is_empty()
    }
}

fn share(k: i64, n: i64) -> f64 {
    if n == 0 { 0.0 } else { k as f64 / n as f64 }
}

fn chi_term(k: i64, n: i64, p: f64) -> f64 {
    let expected = n as f64 * p;
    if expected <= 0.0 {
        return 0.0;
    }
    (k as f64 - expected).powi(2) / expected
}

pub fn tvd_and_chi2(current: &MixWindowRow, baseline: &MixWindowRow) -> (f64, f64) {
    let (nc, nb) = (current.n, baseline.n);
    let mut tvd = 0.0;
    let mut chi2 = 0.0;
    for class in MIX_CLASSES {
        let (kc, kb) = (current.class_count(class), baseline.class_count(class));
        tvd += (share(kc, nc) - share(kb, nb)).abs();
        let pool = share(kc + kb, nc + nb);
        chi2 += chi_term(kc, nc, pool) + chi_term(kb, nb, pool);
    }
    (0.5 * tvd, chi2)
}

fn bundle_counts(row: &MixWindowRow) -> Vec<(String, i64, bool)> {
    row.bundles
        .as_array()
        .map(|items| {
            items
                .iter()
                .map(|b| {
                    (
                        b.get("label")
                            .and_then(Value::as_str)
                            .unwrap_or_default()
                            .to_owned(),
                        b.get("count").and_then(Value::as_i64).unwrap_or(0),
                        b.get("gated").and_then(Value::as_bool).unwrap_or(false),
                    )
                })
                .collect()
        })
        .unwrap_or_default()
}

pub fn explained_by(current: &MixWindowRow, baseline: &MixWindowRow) -> Vec<String> {
    let major: Vec<(String, i64, bool)> = bundle_counts(current)
        .into_iter()
        .filter(|(_, k, _)| share(*k, current.n) >= BUNDLE_SHARE_FLOOR)
        .collect();
    if major.is_empty() || major.iter().any(|(_, _, gated)| !gated) {
        return Vec::new();
    }
    let baseline_labels: Vec<String> = bundle_counts(baseline)
        .into_iter()
        .map(|(l, _, _)| l)
        .collect();
    let new_bundle = major
        .iter()
        .any(|(label, _, _)| !baseline_labels.contains(label));
    if new_bundle {
        major.into_iter().map(|(label, _, _)| label).collect()
    } else {
        Vec::new()
    }
}

pub fn shift_stats(
    current: &MixWindowRow,
    baseline: &MixWindowRow,
    t: &AlarmThresholds,
) -> ShiftStats {
    let (tvd, chi2) = tvd_and_chi2(current, baseline);
    let over_thresholds = current.n >= t.mix_min_turns
        && baseline.n >= BASELINE_MULTIPLE * t.mix_min_turns
        && tvd >= t.mix_min_tvd
        && chi2 >= t.mix_min_chi2;
    ShiftStats {
        n_current: current.n,
        n_baseline: baseline.n,
        tvd,
        chi2,
        over_thresholds,
        explained_by: explained_by(current, baseline),
    }
}

pub fn shift_detail(current: &MixWindowRow, baseline: &MixWindowRow, stats: &ShiftStats) -> Value {
    json!({
        "current": { "n": current.n, "classes": current.classes, "bundles": current.bundles },
        "baseline": { "n": baseline.n, "classes": baseline.classes, "bundles": baseline.bundles },
        "tvd": stats.tvd,
        "chi2": stats.chi2,
        "explained_by": stats.explained_by,
        "inputs": Value::Null,
    })
}

#[cfg(test)]
pub mod tests {
    use super::{AlarmThresholds, shift_stats};
    use crate::store::persistence::eval_metrics::sql_label_mix::MixWindowRow;
    use serde_json::json;

    fn thresholds() -> AlarmThresholds {
        AlarmThresholds {
            mix_min_turns: 50,
            mix_min_tvd: 0.20,
            mix_min_chi2: 40.0,
            backlog_min_turns: 10,
            stalled_min_waiting: 5,
            stalled_hours: 6,
            refusals_min_share: 0.5,
            refusals_min_refused: 50,
            gate_grace_hours: 24,
            call_loop_min_calls: 50,
            call_loop_max_calls_per_row: 3.0,
            session_judge_max_attempts_per_day: 3,
            band_enabled: false,
            band_min: 0.32,
            band_max: 0.49,
            shadow_disagreement_enabled: true,
            shadow_disagreement_min_n: 50,
            shadow_disagreement_min_kappa: 0.80,
            metric_drop_enabled: true,
            metric_drop_min_n: 20,
        }
    }

    fn row(win: &str, classes: serde_json::Value, bundles: serde_json::Value) -> MixWindowRow {
        let n = classes
            .as_object()
            .map_or(0, |m| m.values().filter_map(|v| v.as_i64()).sum());
        MixWindowRow {
            d: chrono::Utc::now(),
            win: win.to_owned(),
            n,
            classes,
            bundles,
            replied_n: 0,
            replied_corrected: 0,
        }
    }

    fn legacy(label: &str, count: i64) -> serde_json::Value {
        json!([{ "label": label, "bundle_id": null, "count": count, "gated": false }])
    }

    #[test]
    fn a_large_shift_fires() {
        let cur = row(
            "current",
            json!({"accepted": 60, "corrected": 5, "abandoned": 120, "shipped": 15}),
            legacy("legacy:b", 200),
        );
        let base = row(
            "baseline",
            json!({"accepted": 300, "corrected": 160, "abandoned": 80, "shipped": 60}),
            legacy("legacy:a", 600),
        );
        let stats = shift_stats(&cur, &base, &thresholds());
        assert!(stats.tvd > 0.2 && stats.chi2 > 40.0, "{stats:?}");
        assert!(stats.fires());
    }

    #[test]
    fn a_stable_mix_is_quiet() {
        let cur = row(
            "current",
            json!({"accepted": 100, "corrected": 20, "abandoned": 60, "shipped": 20}),
            legacy("legacy:a", 200),
        );
        let base = row(
            "baseline",
            json!({"accepted": 300, "corrected": 64, "abandoned": 176, "shipped": 60}),
            legacy("legacy:a", 600),
        );
        assert!(!shift_stats(&cur, &base, &thresholds()).fires());
    }

    #[test]
    fn a_shift_below_min_turns_is_quiet() {
        let cur = row(
            "current",
            json!({"accepted": 5, "corrected": 1, "abandoned": 20, "shipped": 1}),
            legacy("legacy:b", 27),
        );
        let base = row(
            "baseline",
            json!({"accepted": 300, "corrected": 160, "abandoned": 80, "shipped": 60}),
            legacy("legacy:a", 600),
        );
        assert!(!shift_stats(&cur, &base, &thresholds()).fires());
    }

    #[test]
    fn a_shift_from_a_new_gated_bundle_is_explained() {
        let gated = json!([{ "label": "bundle:9", "bundle_id": 9, "count": 200, "gated": true }]);
        let cur = row(
            "current",
            json!({"accepted": 60, "corrected": 5, "abandoned": 120, "shipped": 15}),
            gated,
        );
        let base = row(
            "baseline",
            json!({"accepted": 300, "corrected": 160, "abandoned": 80, "shipped": 60}),
            legacy("legacy:a", 600),
        );
        let stats = shift_stats(&cur, &base, &thresholds());
        assert!(stats.over_thresholds);
        assert_eq!(stats.explained_by, vec!["bundle:9".to_owned()]);
        assert!(!stats.fires());
    }
}
