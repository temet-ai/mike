use serde_json::{Value, json};

use super::questions::{applies_key, key, requires_key};
use super::rules::Rule;
use crate::llm::decisions::point::Asked;

pub const WARN_AT: f64 = 0.9;
pub const HOLD_AT: f64 = 0.3;
pub const APPLIES_AT: f64 = 0.75;

#[derive(Debug, Clone, PartialEq)]
pub struct Outcome {
    pub rule: usize,
    pub probability: f64,
    pub violated: bool,
    pub held: bool,
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
            "held": self.held,
            "warn_at": WARN_AT,
            "hold_at": HOLD_AT,
            "applies_at": APPLIES_AT,
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

fn chance(asked: &Asked, index: usize) -> Option<f64> {
    if asked.noul(&applies_key(index))? < APPLIES_AT {
        return Some(0.0);
    }
    let forbids = asked.noul(&key(index))?;
    Some(
        asked
            .noul(&requires_key(index))
            .map_or(forbids, |leaves_out| forbids.max(leaves_out)),
    )
}

pub fn compose(asked: &Asked, count: usize) -> Option<Outcome> {
    if asked.failed() {
        return None;
    }
    let chances: Vec<f64> = (0..count)
        .map(|i| chance(asked, i))
        .collect::<Option<_>>()?;
    let (rule, probability) = chances
        .iter()
        .copied()
        .enumerate()
        .max_by(|a, b| a.1.total_cmp(&b.1))?;
    Some(Outcome {
        rule,
        probability,
        violated: probability >= WARN_AT,
        held: probability <= HOLD_AT,
    })
}
