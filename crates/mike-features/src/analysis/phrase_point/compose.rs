use super::questions::key;
use crate::llm::decisions::point::Asked;

pub const EXCUSE_AT: f64 = 0.5;

pub fn compose(asked: &Asked, count: usize) -> Option<Vec<bool>> {
    if asked.failed() {
        return None;
    }
    (0..count)
        .map(|i| asked.noul(&key(i)).map(|p| p >= EXCUSE_AT))
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

pub fn keep<T>(items: Vec<T>, excuses: &[bool]) -> Vec<T> {
    items
        .into_iter()
        .enumerate()
        .filter(|(i, _)| excuses.get(*i).copied().unwrap_or(true))
        .map(|(_, item)| item)
        .collect()
}
