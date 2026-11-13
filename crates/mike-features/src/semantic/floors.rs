use std::collections::HashMap;

use serde::Serialize;

pub const FLOOR_BUILDER_VERSION: &str = "floors-v1";
pub const MIN_PAIRS: i64 = 10_000;
pub const MAX_PAIRS: i64 = 40_000;

pub const SIMILAR_CORRECTIONS: [&str; 3] = ["prompt:corr", "ending:corr", "ending:ending"];
pub const LABEL_FREE_EXCHANGES: [&str; 2] = ["prompt:prompt", "ending:ending"];
pub const TAGGER: [&str; 1] = ["corr:corr"];
pub const DIRECTIVES: [&str; 2] = ["prompt:directive", "ending:directive"];
pub const LESSONS: [&str; 2] = ["prompt:lesson", "ending:lesson"];

pub fn all_pairs() -> Vec<&'static str> {
    let mut pairs: Vec<&'static str> = SIMILAR_CORRECTIONS
        .iter()
        .chain(&LABEL_FREE_EXCHANGES)
        .chain(&TAGGER)
        .chain(&DIRECTIVES)
        .chain(&LESSONS)
        .copied()
        .collect();
    pairs.sort_unstable();
    pairs.dedup();
    pairs
}

pub fn split_pair(pair: &str) -> Option<(&str, &str)> {
    pair.split_once(':')
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Floor {
    pub id: i64,
    pub floor: f64,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct FloorSet {
    pub by_pair: HashMap<String, Floor>,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize)]
pub struct Scored {
    pub similarity: f64,
    pub score: f64,
    pub floor_id: i64,
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum FloorError {
    #[error("no floor in force for pair {0}")]
    NoFloor(String),
}

impl FloorSet {
    pub fn score(&self, pair: &str, similarity: f64) -> Result<Scored, FloorError> {
        let floor = self
            .by_pair
            .get(pair)
            .ok_or_else(|| FloorError::NoFloor(pair.to_owned()))?;
        Ok(Scored {
            similarity,
            score: similarity - floor.floor,
            floor_id: floor.id,
        })
    }

    pub fn covers_any(&self, pairs: &[&str]) -> bool {
        pairs.iter().any(|p| self.by_pair.contains_key(*p))
    }

    pub fn ids(&self) -> Vec<i64> {
        let mut ids: Vec<i64> = self.by_pair.values().map(|f| f.id).collect();
        ids.sort_unstable();
        ids
    }
}

pub fn best_allowed(
    set: &FloorSet,
    candidates: &[(&str, f64)],
    allowed: &[&str],
) -> Option<(String, Scored)> {
    candidates
        .iter()
        .filter(|(pair, _)| allowed.contains(pair))
        .filter_map(|(pair, sim)| set.score(pair, *sim).ok().map(|s| ((*pair).to_owned(), s)))
        .max_by(|a, b| a.1.score.total_cmp(&b.1.score))
}

pub fn percentile(sorted: &[f64], q: f64) -> Option<f64> {
    if sorted.is_empty() {
        return None;
    }
    let rank = q.clamp(0.0, 1.0) * (sorted.len() - 1) as f64;
    let (lo, hi) = (rank.floor() as usize, rank.ceil() as usize);
    let frac = rank - lo as f64;
    Some(sorted[lo] + (sorted[hi] - sorted[lo]) * frac)
}

pub fn floor_status(n_pairs: i64, first_for_key: bool) -> &'static str {
    if n_pairs < MIN_PAIRS {
        "insufficient"
    } else if first_for_key {
        "in_force"
    } else {
        "monitoring"
    }
}

pub fn promotion_refused(pair: &str, llm_facing_pairs: &[&str], rebundle: bool) -> bool {
    !rebundle && llm_facing_pairs.contains(&pair)
}
