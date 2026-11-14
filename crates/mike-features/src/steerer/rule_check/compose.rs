use serde_json::{Value, json};

use super::questions::key;
use super::rules::Rule;
use crate::llm::decisions::point::Asked;

pub const VIOLATION_AT: f64 = 0.8;

#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub rule: usize,
    pub probability: f64,
    pub violated: bool,
}

impl Outcome {
    pub fn score(&self) -> f64 {
        if self.violated {
            self.probability
        } else {
            1.0 - self.probability
        }
    }

    pub fn json(&self, delivered: bool) -> Value {
        json!({
            "score": self.score(),
            "rule": self.rule,
            "probability": self.probability,
            "violated": self.violated,
            "violation_at": VIOLATION_AT,
            "delivered": delivered,
        })
    }

    pub fn note(&self, rule: &Rule) -> String {
        format!(
            "{} says: \"{}\" This turn may conflict with it. If that was deliberate, say why; otherwise bring the work back in line.",
            rule.source, rule.text
        )
    }
}

pub fn compose(asked: &Asked, count: usize) -> Option<Outcome> {
    if asked.failed() {
        return None;
    }
    let chances: Vec<f64> = (0..count)
        .map(|i| asked.noul(&key(i)))
        .collect::<Option<_>>()?;
    let (rule, probability) = chances
        .iter()
        .copied()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    Some(Outcome {
        rule,
        probability,
        violated: probability >= VIOLATION_AT,
    })
}
