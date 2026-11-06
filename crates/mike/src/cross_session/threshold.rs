use chrono::{DateTime, Duration, Utc};
use serde::Serialize;

pub const Z: f64 = 1.96;

pub fn wilson_lower_bound(k: f64, n: f64) -> Option<f64> {
    if n <= 0.0 {
        return None;
    }
    let p = k / n;
    let z2 = Z * Z;
    let centre = p + z2 / (2.0 * n);
    let half = Z * (p * (1.0 - p) / n + z2 / (4.0 * n * n)).sqrt();
    Some(((centre - half) / (1.0 + z2 / n)).max(0.0))
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Bin {
    pub lower_edge: f64,
    pub k: f64,
    pub n: f64,
}

pub fn calibrate_threshold(
    bins: &[Bin],
    bar: f64,
    floor: f64,
    ceiling: f64,
    min_n: f64,
) -> Option<f32> {
    let mut sorted: Vec<Bin> = bins.to_vec();
    sorted.sort_by(|a, b| b.lower_edge.total_cmp(&a.lower_edge));
    let (mut k, mut n) = (0.0, 0.0);
    let mut candidate = None;
    for bin in sorted {
        k += bin.k;
        n += bin.n;
        if n >= min_n && wilson_lower_bound(k, n).is_some_and(|l| l >= bar) {
            candidate = Some(bin.lower_edge);
        }
    }
    candidate.map(|c| c.clamp(floor, ceiling) as f32)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Decision {
    ModeOff,
    Insufficient,
    BelowBar,
    HeldHysteresis,
    HeldCooldown,
    HeldWakeFloor,
    ShadowOnly,
    Applied,
}

impl Decision {
    pub fn code(self) -> &'static str {
        match self {
            Self::ModeOff => "mode_off",
            Self::Insufficient => "insufficient",
            Self::BelowBar => "below_bar",
            Self::HeldHysteresis => "held_hysteresis",
            Self::HeldCooldown => "held_cooldown",
            Self::HeldWakeFloor => "held_wake_floor",
            Self::ShadowOnly => "shadow_only",
            Self::Applied => "applied",
        }
    }
}

pub struct DecisionInputs<'a> {
    pub current: f32,
    pub static_threshold: f32,
    pub candidate: Option<f32>,
    pub enough_labelled: bool,
    pub mode: &'a str,
    pub hysteresis: f64,
    pub cooldown_days: i64,
    pub last_applied_at: Option<DateTime<Utc>>,
    pub question_mode: &'a str,
    pub allow_lowering_for_wake: bool,
    pub now: DateTime<Utc>,
}

pub fn decide_threshold(i: &DecisionInputs<'_>) -> Decision {
    if i.mode == "off" {
        return Decision::ModeOff;
    }
    let Some(candidate) = i.candidate else {
        return if i.enough_labelled {
            Decision::BelowBar
        } else {
            Decision::Insufficient
        };
    };
    if f64::from((candidate - i.current).abs()) < i.hysteresis {
        return Decision::HeldHysteresis;
    }
    if i.last_applied_at
        .is_some_and(|at| i.now - at < Duration::days(i.cooldown_days))
    {
        return Decision::HeldCooldown;
    }
    if i.question_mode == "wake" && candidate < i.static_threshold && !i.allow_lowering_for_wake {
        return Decision::HeldWakeFloor;
    }
    if i.mode == "shadow" {
        return Decision::ShadowOnly;
    }
    Decision::Applied
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bins() -> Vec<Bin> {
        vec![
            Bin {
                lower_edge: 0.95,
                k: 10.0,
                n: 10.0,
            },
            Bin {
                lower_edge: 0.90,
                k: 18.0,
                n: 20.0,
            },
            Bin {
                lower_edge: 0.85,
                k: 15.0,
                n: 20.0,
            },
            Bin {
                lower_edge: 0.80,
                k: 5.0,
                n: 20.0,
            },
        ]
    }

    #[test]
    fn wilson_lower_bound_known_values() {
        assert!(wilson_lower_bound(34.0, 40.0).is_some_and(|l| (l - 0.709).abs() < 0.002));
        assert_eq!(wilson_lower_bound(0.0, 0.0), None);
    }

    #[test]
    fn calibrate_threshold_walks_down_to_the_lowest_edge_that_clears_the_bar() {
        let t = calibrate_threshold(&bins(), 0.75, 0.6, 0.95, 20.0);
        assert_eq!(t, Some(0.9));
        assert_eq!(calibrate_threshold(&bins(), 0.99, 0.6, 0.95, 20.0), None);
        assert_eq!(calibrate_threshold(&bins(), 0.5, 0.6, 0.95, 5.0), Some(0.8));
        assert_eq!(
            calibrate_threshold(&bins(), 0.5, 0.85, 0.95, 5.0),
            Some(0.85)
        );
    }

    #[test]
    fn decide_threshold_hysteresis_cooldown_and_wake_floor() {
        let now = Utc::now();
        let base = DecisionInputs {
            current: 0.75,
            static_threshold: 0.75,
            candidate: Some(0.9),
            enough_labelled: true,
            mode: "apply",
            hysteresis: 0.05,
            cooldown_days: 7,
            last_applied_at: None,
            question_mode: "shadow",
            allow_lowering_for_wake: false,
            now,
        };
        assert_eq!(decide_threshold(&base), Decision::Applied);
        assert_eq!(
            decide_threshold(&DecisionInputs {
                candidate: Some(0.77),
                ..base
            }),
            Decision::HeldHysteresis
        );
        assert_eq!(
            decide_threshold(&DecisionInputs {
                last_applied_at: Some(now - Duration::days(2)),
                ..base
            }),
            Decision::HeldCooldown
        );
        assert_eq!(
            decide_threshold(&DecisionInputs {
                candidate: Some(0.62),
                question_mode: "wake",
                ..base
            }),
            Decision::HeldWakeFloor
        );
        assert_eq!(
            decide_threshold(&DecisionInputs {
                mode: "off",
                ..base
            }),
            Decision::ModeOff
        );
        assert_eq!(
            decide_threshold(&DecisionInputs {
                candidate: None,
                enough_labelled: false,
                ..base
            }),
            Decision::Insufficient
        );
        assert_eq!(
            decide_threshold(&DecisionInputs {
                candidate: None,
                ..base
            }),
            Decision::BelowBar
        );
    }
}
