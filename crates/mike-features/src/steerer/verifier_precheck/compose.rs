use serde_json::{Value, json};

use super::questions::{SENDS_WHEN_FALSE, SENDS_WHEN_TRUE};
use crate::llm::decisions::point::Asked;

pub const HOLD_BELOW: f64 = 0.3;

#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub send: bool,
    pub score: f64,
}

impl Verdict {
    pub fn json(&self) -> Value {
        let right = if self.send {
            self.score
        } else {
            1.0 - self.score
        };
        json!({
            "score": right,
            "send_score": self.score,
            "send": self.send,
            "hold_below": HOLD_BELOW,
        })
    }
}

pub fn compose(asked: &Asked) -> Option<Verdict> {
    if asked.failed() {
        return None;
    }
    let mut total = 0.0;
    for key in SENDS_WHEN_TRUE {
        total += asked.noul(key)?;
    }
    for key in SENDS_WHEN_FALSE {
        total += 1.0 - asked.noul(key)?;
    }
    let score = total / (SENDS_WHEN_TRUE.len() + SENDS_WHEN_FALSE.len()) as f64;
    Some(Verdict {
        send: score >= HOLD_BELOW,
        score,
    })
}
