use serde::Serialize;

pub const READ_POINT_N: i64 = 20;
const Z: f64 = 1.959964;
const Z2: f64 = 3.8414588;

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct WilsonInterval {
    pub low: f64,
    pub high: f64,
}

pub fn wilson_interval(k: i64, n: i64) -> Option<WilsonInterval> {
    if n <= 0 || k < 0 || k > n {
        return None;
    }
    let nf = n as f64;
    let p = k as f64 / nf;
    let denom = 1.0 + Z2 / nf;
    let center = p + Z2 / (2.0 * nf);
    let margin = Z * ((p * (1.0 - p) / nf) + Z2 / (4.0 * nf * nf)).sqrt();
    let low = ((center - margin) / denom).max(0.0);
    let high = ((center + margin) / denom).min(1.0);
    Some(WilsonInterval { low, high })
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct NewcombeInterval {
    pub diff: f64,
    pub low: f64,
    pub high: f64,
}

pub fn newcombe_interval(k1: i64, n1: i64, k2: i64, n2: i64) -> Option<NewcombeInterval> {
    let w1 = wilson_interval(k1, n1)?;
    let w2 = wilson_interval(k2, n2)?;
    let p1 = k1 as f64 / n1 as f64;
    let p2 = k2 as f64 / n2 as f64;
    let diff = p2 - p1;
    let low = diff - ((p2 - w2.low).powi(2) + (w1.high - p1).powi(2)).sqrt();
    let high = diff + ((w2.high - p2).powi(2) + (p1 - w1.low).powi(2)).sqrt();
    Some(NewcombeInterval { diff, low, high })
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum EffectReadiness {
    NotEnoughData {
        n_before: i64,
        n_after: i64,
        read_point: i64,
    },
    Ready(NewcombeInterval),
}

fn not_enough_data(n_before: i64, n_after: i64) -> EffectReadiness {
    EffectReadiness::NotEnoughData {
        n_before,
        n_after,
        read_point: READ_POINT_N,
    }
}

pub fn effect_readiness(k1: i64, n1: i64, k2: i64, n2: i64) -> EffectReadiness {
    if n1 < READ_POINT_N || n2 < READ_POINT_N {
        return not_enough_data(n1, n2);
    }
    match newcombe_interval(k1, n1, k2, n2) {
        Some(interval) => EffectReadiness::Ready(interval),
        None => not_enough_data(n1, n2),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn wilson_interval_of_zero_successes_has_a_zero_lower_bound() {
        let Some(interval) = wilson_interval(0, 10) else {
            panic!("n>0");
        };
        assert!(interval.low.abs() < 1e-9);
        assert!((interval.high - 0.277_532_8).abs() < 1e-6);
    }

    #[test]
    fn wilson_interval_rejects_non_positive_n() {
        assert_eq!(wilson_interval(0, 0), None);
        assert_eq!(wilson_interval(1, -1), None);
    }

    #[test]
    fn newcombe_interval_of_two_identical_zero_arms_is_symmetric_around_zero() {
        let Some(interval) = newcombe_interval(0, 10, 0, 10) else {
            panic!("both arms valid");
        };
        assert!(interval.diff.abs() < 1e-9);
        assert!((interval.low + 0.277_532_8).abs() < 1e-6);
        assert!((interval.high - 0.277_532_8).abs() < 1e-6);
    }

    #[test]
    fn newcombe_interval_brackets_the_point_difference() {
        let Some(interval) = newcombe_interval(40, 100, 20, 100) else {
            panic!("both arms valid");
        };
        assert!((interval.diff - (-0.2)).abs() < 1e-9);
        assert!(interval.low <= interval.diff);
        assert!(interval.high >= interval.diff);
        assert!(interval.low < 0.0);
    }

    #[test]
    fn newcombe_interval_matches_newcombe_1998_method_10_worked_example() {
        let Some(interval) = newcombe_interval(56, 70, 48, 80) else {
            panic!("both arms valid");
        };
        assert!((interval.diff - (-0.2)).abs() < 1e-9);
        assert!((interval.low - (-0.333_872_66)).abs() < 1e-6);
        assert!((interval.high - (-0.052_431_47)).abs() < 1e-6);
    }

    #[test]
    fn effect_readiness_reports_not_enough_data_below_the_read_point() {
        let readiness = effect_readiness(1, 5, 10, 50);
        assert_eq!(
            readiness,
            EffectReadiness::NotEnoughData {
                n_before: 5,
                n_after: 50,
                read_point: READ_POINT_N,
            }
        );
    }

    #[test]
    fn effect_readiness_returns_an_interval_at_or_above_the_read_point() {
        let readiness = effect_readiness(4, 20, 2, 20);
        match readiness {
            EffectReadiness::Ready(interval) => assert!(interval.diff < 0.0),
            EffectReadiness::NotEnoughData { .. } => panic!("expected Ready"),
        }
    }
}
