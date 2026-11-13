use serde::{Deserialize, Serialize};

pub const DEFAULT_RESCUE_SCORE: f64 = -1.25;

pub const DEFAULT_RESCUE_MIN_KEPT: usize = 3;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct RerankNightly {
    pub start_hour_utc: u32,
    pub end_hour_utc: u32,
    pub max_load_per_cpu: f64,
    pub lookback_days: i64,
    pub min_graded_searches: usize,
    pub min_gain: f64,
    pub lesson_pair_min_score: f64,
    pub related_per_thread: usize,
}

impl Default for RerankNightly {
    fn default() -> Self {
        Self {
            start_hour_utc: 22,
            end_hour_utc: 4,
            max_load_per_cpu: 0.5,
            lookback_days: 14,
            min_graded_searches: crate::search_ranking::tuning::MIN_GRADED_SEARCHES,
            min_gain: crate::search_ranking::tuning::MIN_GAIN,
            lesson_pair_min_score: 0.5,
            related_per_thread: 10,
        }
    }
}

impl RerankNightly {
    pub fn assign(&mut self, key: &str, value: &serde_json::Value) -> Option<bool> {
        let int = value.as_i64();
        let applied = match key {
            "rerank.nightly.start_hour_utc" => set(&mut self.start_hour_utc, hour(int)),
            "rerank.nightly.end_hour_utc" => set(&mut self.end_hour_utc, hour(int)),
            "rerank.nightly.max_load_per_cpu" => set(&mut self.max_load_per_cpu, value.as_f64()),
            "rerank.tuning.lookback_days" => set(&mut self.lookback_days, int.map(|v| v.max(1))),
            "rerank.tuning.min_graded_searches" => set(&mut self.min_graded_searches, count(int)),
            "rerank.tuning.min_gain" => set(&mut self.min_gain, value.as_f64()),
            "rerank.lesson_pairs.min_score" => set(&mut self.lesson_pair_min_score, value.as_f64()),
            "rerank.related.per_thread" => set(&mut self.related_per_thread, count(int)),
            _ => return None,
        };
        Some(applied)
    }

    pub fn in_window(&self, hour: u32) -> bool {
        let (start, end) = (self.start_hour_utc, self.end_hour_utc);
        match start.cmp(&end) {
            std::cmp::Ordering::Less => hour >= start && hour < end,
            std::cmp::Ordering::Greater => hour >= start || hour < end,
            std::cmp::Ordering::Equal => false,
        }
    }
}

fn hour(value: Option<i64>) -> Option<u32> {
    value.and_then(|v| u32::try_from(v.clamp(0, 23)).ok())
}

fn count(value: Option<i64>) -> Option<usize> {
    value.and_then(|v| usize::try_from(v.max(1)).ok())
}

fn set<T>(slot: &mut T, value: Option<T>) -> bool {
    match value {
        Some(v) => {
            *slot = v;
            true
        }
        None => false,
    }
}
