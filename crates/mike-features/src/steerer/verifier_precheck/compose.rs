use serde_json::{Value, json};

use super::questions::{UNFINISHED, VETOES};
use crate::llm::decisions::point::Asked;

pub const HOLD_BELOW: f64 = 0.8;
pub const VETO_AT: f64 = 0.6;

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
            "veto_at": VETO_AT,
        })
    }
}

pub fn compose(asked: &Asked) -> Option<Verdict> {
    if asked.failed() {
        return None;
    }
    let unfinished = asked.noul(UNFINISHED)?;
    let veto = VETOES
        .iter()
        .filter_map(|key| asked.noul(key))
        .fold(0.0_f64, f64::max);
    Some(Verdict {
        send: unfinished >= HOLD_BELOW && veto < VETO_AT,
        score: unfinished.min(1.0 - veto),
    })
}
