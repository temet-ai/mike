use serde::Serialize;

use super::embedder::cosine;

pub const DRIFT_VERSION: &str = "drift-v1";
pub const PRIOR_DAYS: usize = 28;
pub const MIN_PRIOR_DAYS: usize = 7;
pub const WARMING_DAYS: i64 = 35;
pub const QUIET_Z: f64 = 2.0;
pub const STREAMS: [&str; 3] = ["prompt", "ending", "correction"];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum DriftStatus {
    Scored,
    Warming,
    TooFewItems,
}

impl DriftStatus {
    pub fn code(self) -> &'static str {
        match self {
            Self::Scored => "scored",
            Self::Warming => "warming",
            Self::TooFewItems => "too_few_items",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct DriftPoint {
    pub distance: Option<f64>,
    pub z: Option<f64>,
    pub prior_days: usize,
    pub prior_mean: Option<f64>,
    pub prior_sd: Option<f64>,
    pub status: DriftStatus,
}

pub fn centroid_distance(current_sum: &[f32], baseline_sum: &[f32]) -> f64 {
    1.0 - cosine(current_sum, baseline_sum)
}

pub fn z_score(distance: f64, prior: &[f64]) -> (Option<f64>, Option<f64>, Option<f64>) {
    if prior.len() < MIN_PRIOR_DAYS {
        return (None, None, None);
    }
    let n = prior.len() as f64;
    let mean = prior.iter().sum::<f64>() / n;
    let var = prior.iter().map(|d| (d - mean).powi(2)).sum::<f64>() / (n - 1.0).max(1.0);
    let sd = var.sqrt();
    let z = if sd > 0.0 {
        Some((distance - mean) / sd)
    } else {
        None
    };
    (z, Some(mean), Some(sd))
}

pub struct DayInputs<'a> {
    pub current_sum: Option<&'a [f32]>,
    pub baseline_sum: Option<&'a [f32]>,
    pub n_current: i64,
    pub min_current: i64,
    pub prior: &'a [f64],
    pub embedder_age_days: i64,
}

pub fn score_day(d: &DayInputs<'_>) -> DriftPoint {
    let empty = |status| DriftPoint {
        distance: None,
        z: None,
        prior_days: d.prior.len(),
        prior_mean: None,
        prior_sd: None,
        status,
    };
    let (Some(cur), Some(base)) = (d.current_sum, d.baseline_sum) else {
        return empty(DriftStatus::TooFewItems);
    };
    if d.n_current < d.min_current {
        return empty(DriftStatus::TooFewItems);
    }
    if d.embedder_age_days < WARMING_DAYS {
        return empty(DriftStatus::Warming);
    }
    let distance = centroid_distance(cur, base);
    let (z, prior_mean, prior_sd) = z_score(distance, d.prior);
    DriftPoint {
        distance: Some(distance),
        z,
        prior_days: d.prior.len(),
        prior_mean,
        prior_sd,
        status: DriftStatus::Scored,
    }
}

pub fn fires(point: &DriftPoint, z_threshold: f64) -> bool {
    point.status == DriftStatus::Scored && point.z.is_some_and(|z| z >= z_threshold)
}

pub fn min_current_for(stream: &str, base: i64) -> i64 {
    if stream == "correction" {
        (base / 3).max(1)
    } else {
        base
    }
}
