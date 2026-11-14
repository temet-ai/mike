use serde_json::{Value, json};

use super::questions::CORRECTS;
use crate::llm::decisions::point::Asked;

pub const CORRECTION_AT: f64 = 0.75;

#[derive(Debug, Clone, PartialEq)]
pub struct Classified {
    pub kind: &'static str,
    pub probability: f64,
}

impl Classified {
    pub fn json(&self) -> Value {
        let right = if self.kind == "correction" {
            self.probability
        } else {
            1.0 - self.probability
        };
        json!({
            "score": right,
            "kind": self.kind,
            "probability": self.probability,
            "correction_at": CORRECTION_AT,
        })
    }
}

pub fn compose(asked: &Asked) -> Option<Classified> {
    if asked.failed() {
        return None;
    }
    let probability = asked.noul(CORRECTS)?;
    Some(Classified {
        kind: if probability >= CORRECTION_AT {
            "correction"
        } else {
            "clarification"
        },
        probability,
    })
}
