use serde::Serialize;
use serde_json::{Value, json};

use super::sql_label_mix::MixWindowRow;
use super::types::AlarmThresholds;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct BandStats {
    pub n: i64,
    pub k: i64,
    pub share: f64,
    pub min: f64,
    pub max: f64,
    pub enabled: bool,
    pub have_enough_turns: bool,
    pub in_band: bool,
}

impl BandStats {
    pub fn fires(&self) -> bool {
        self.enabled && self.have_enough_turns && !self.in_band
    }
}

fn share(k: i64, n: i64) -> f64 {
    if n == 0 { 0.0 } else { k as f64 / n as f64 }
}

pub fn band_stats(current: &MixWindowRow, t: &AlarmThresholds) -> BandStats {
    let n = current.replied_n;
    let k = current.replied_corrected;
    let s = share(k, n);
    BandStats {
        n,
        k,
        share: s,
        min: t.band_min,
        max: t.band_max,
        enabled: t.band_enabled,
        have_enough_turns: n >= t.mix_min_turns,
        in_band: s >= t.band_min && s <= t.band_max,
    }
}

pub fn band_detail(stats: &BandStats) -> Value {
    json!({
        "n": stats.n,
        "k": stats.k,
        "share": stats.share,
        "min": stats.min,
        "max": stats.max,
        "enabled": stats.enabled,
        "window_days": 7,
    })
}

#[cfg(test)]
pub mod tests {
    use super::{AlarmThresholds, band_stats};
    use crate::store::persistence::eval_metrics::sql_label_mix::MixWindowRow;
    use serde_json::json;

    fn thresholds(enabled: bool) -> AlarmThresholds {
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
            band_enabled: enabled,
            band_min: 0.32,
            band_max: 0.49,
            shadow_disagreement_enabled: true,
            shadow_disagreement_min_n: 50,
            shadow_disagreement_min_kappa: 0.80,
            metric_drop_enabled: true,
            metric_drop_min_n: 20,
        }
    }

    fn current_window(replied_n: i64, replied_corrected: i64) -> MixWindowRow {
        MixWindowRow {
            d: chrono::Utc::now(),
            win: "current".to_owned(),
            n: replied_n,
            classes: json!({}),
            bundles: json!([]),
            replied_n,
            replied_corrected,
        }
    }

    #[test]
    fn a_share_inside_the_operator_census_band_is_quiet() {
        let row = current_window(200, 80);
        let stats = band_stats(&row, &thresholds(true));
        assert!((stats.share - 0.40).abs() < 1e-9);
        assert!(stats.in_band);
        assert!(!stats.fires());
    }

    #[test]
    fn a_share_below_the_band_fires() {
        let row = current_window(200, 20);
        let stats = band_stats(&row, &thresholds(true));
        assert!(stats.share < 0.32);
        assert!(!stats.in_band);
        assert!(stats.fires());
    }

    #[test]
    fn a_share_above_the_band_fires() {
        let row = current_window(200, 140);
        let stats = band_stats(&row, &thresholds(true));
        assert!(stats.share > 0.49);
        assert!(!stats.in_band);
        assert!(stats.fires());
    }

    #[test]
    fn disabled_never_fires_even_when_the_share_is_outside_the_band() {
        let row = current_window(200, 20);
        let stats = band_stats(&row, &thresholds(false));
        assert!(!stats.in_band);
        assert!(!stats.fires());
    }

    #[test]
    fn below_min_turns_does_not_fire_even_when_the_share_is_outside_the_band() {
        let row = current_window(10, 1);
        let stats = band_stats(&row, &thresholds(true));
        assert!(!stats.have_enough_turns);
        assert!(!stats.fires());
    }
}
