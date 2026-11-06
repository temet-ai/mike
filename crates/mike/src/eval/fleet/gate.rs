use serde::Serialize;
use serde_json::{Value, json};

use super::numbers::{FleetNumbers, METRIC_NAMES};
use crate::eval::gate::{EXIT_FAIL, EXIT_INCOMPLETE, EXIT_PASS};

pub const DEFAULT_TOLERANCE: f64 = 0.10;
pub const SIGMA_MULTIPLE: f64 = 2.0;
pub const RUNS_FOR_SIGMA: usize = 10;

#[derive(Debug, Clone, PartialEq)]
pub enum NoiseBand {
    Fraction(f64),
    Sigma([f64; 4]),
}

impl NoiseBand {
    pub fn from_history(history: &[FleetNumbers]) -> Self {
        if history.len() < RUNS_FOR_SIGMA {
            return Self::Fraction(DEFAULT_TOLERANCE);
        }
        let mut sigmas = [0.0; 4];
        for (slot, name) in sigmas.iter_mut().zip(METRIC_NAMES) {
            let values: Vec<f64> = history.iter().filter_map(|n| n.get(name)).collect();
            *slot = standard_deviation(&values);
        }
        Self::Sigma(sigmas)
    }

    fn allowance(&self, index: usize, before: f64) -> f64 {
        match self {
            Self::Fraction(fraction) => before.abs() * fraction,
            Self::Sigma(sigmas) => SIGMA_MULTIPLE * sigmas.get(index).copied().unwrap_or(0.0),
        }
    }

    fn describe(&self) -> Value {
        match self {
            Self::Fraction(fraction) => json!({ "kind": "fraction", "fraction": fraction }),
            Self::Sigma(sigmas) => json!({ "kind": "two_sigma", "sigma": sigmas }),
        }
    }
}

fn standard_deviation(values: &[f64]) -> f64 {
    if values.len() < 2 {
        return 0.0;
    }
    let mean = values.iter().sum::<f64>() / values.len() as f64;
    let variance =
        values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / (values.len() - 1) as f64;
    variance.sqrt()
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MetricComparison {
    pub metric: &'static str,
    pub before: f64,
    pub after: f64,
    pub allowance: f64,
    pub worsened: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FleetGateDecision {
    pub verdict: &'static str,
    pub exit_code: i32,
    pub report: Value,
}

fn incomplete(reason: String) -> FleetGateDecision {
    FleetGateDecision {
        verdict: "incomplete",
        exit_code: EXIT_INCOMPLETE,
        report: json!({ "incomplete": reason }),
    }
}

fn compare(before: &FleetNumbers, after: &FleetNumbers, band: &NoiseBand) -> Vec<MetricComparison> {
    METRIC_NAMES
        .into_iter()
        .enumerate()
        .filter_map(|(index, metric)| {
            let (b, a) = (before.get(metric)?, after.get(metric)?);
            let allowance = band.allowance(index, b);
            Some(MetricComparison {
                metric,
                before: b,
                after: a,
                allowance,
                worsened: a - b > allowance,
            })
        })
        .collect()
}

pub fn decide(before: &FleetNumbers, after: &FleetNumbers, band: &NoiseBand) -> FleetGateDecision {
    let missing_before = before.missing();
    let missing_after = after.missing();
    if !missing_before.is_empty() || !missing_after.is_empty() {
        return incomplete(format!(
            "record lacks numbers (before {missing_before:?}, after {missing_after:?})"
        ));
    }
    let table = compare(before, after, band);
    let worsened: Vec<&str> = table
        .iter()
        .filter(|c| c.worsened)
        .map(|c| c.metric)
        .collect();
    let (verdict, exit_code) = if worsened.is_empty() {
        ("pass", EXIT_PASS)
    } else {
        ("fail", EXIT_FAIL)
    };
    FleetGateDecision {
        verdict,
        exit_code,
        report: json!({ "band": band.describe(), "metrics": table, "worsened": worsened }),
    }
}
