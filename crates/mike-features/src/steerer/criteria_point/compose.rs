use super::questions::key;
use crate::llm::decisions::point::Asked;

pub const ACCEPT_AT: f64 = 0.5;

pub fn compose(asked: &Asked, count: usize) -> Option<Vec<bool>> {
    if asked.failed() {
        return None;
    }
    (0..count)
        .map(|i| asked.noul(&key(i)).map(|p| p >= ACCEPT_AT))
        .collect()
}

pub fn mean_confidence(asked: &Asked, count: usize) -> f64 {
    if count == 0 {
        return 0.0;
    }
    let total: f64 = (0..count)
        .filter_map(|i| asked.noul(&key(i)))
        .map(|p| p.max(1.0 - p))
        .sum();
    total / count as f64
}
