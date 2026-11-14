use serde_json::{Value, json};

use super::questions::UNFINISHED;
use crate::llm::decisions::point::Asked;

pub const HOLD_BELOW: f64 = 0.8;

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
    let score = asked.noul(UNFINISHED)?;
    Some(Verdict {
        send: score >= HOLD_BELOW,
        score,
    })
}
