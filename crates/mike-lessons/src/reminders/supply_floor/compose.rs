use serde_json::{Value, json};

use super::questions::key;
use crate::llm::decisions::point::Asked;

pub const FLOOR: f64 = 0.5;

#[derive(Debug, Clone, PartialEq)]
pub struct Supply {
    pub keep: Vec<i64>,
    pub hold: Vec<i64>,
    pub score: f64,
    pub chances: Vec<(i64, f64)>,
}

impl Supply {
    pub fn json(&self) -> Value {
        json!({
            "score": self.score,
            "keep": self.keep,
            "hold": self.hold,
            "chances": self.chances,
            "floor": FLOOR,
        })
    }
}

pub fn compose(asked: &Asked, ids: &[i64]) -> Option<Supply> {
    if asked.failed() {
        return None;
    }
    let chances: Vec<(i64, f64)> = ids
        .iter()
        .map(|id| asked.noul(&key(*id)).map(|p| (*id, p)))
        .collect::<Option<_>>()?;
    let keep = chances
        .iter()
        .filter(|(_, p)| *p >= FLOOR)
        .map(|(id, _)| *id)
        .collect();
    let hold = chances
        .iter()
        .filter(|(_, p)| *p < FLOOR)
        .map(|(id, _)| *id)
        .collect();
    let right: f64 = chances
        .iter()
        .map(|(_, p)| if *p >= FLOOR { *p } else { 1.0 - *p })
        .sum();
    Some(Supply {
        keep,
        hold,
        score: right / chances.len().max(1) as f64,
        chances,
    })
}
