use serde_json::{Value, json};

use super::questions::{CRITERION_GOAL, UNFINISHED, VETOES};
use crate::llm::decisions::point::Asked;

pub const HOLD_BELOW: f64 = 0.8;
pub const VETO_AT: f64 = 0.6;

#[derive(Debug, Clone, PartialEq)]
pub struct Verdict {
    pub send: bool,
    pub score: f64,
    pub rule: Option<&'static str>,
    pub holdback: bool,
}

impl Verdict {
    pub fn held_by(rule: &'static str) -> Self {
        Self {
            send: false,
            score: 0.0,
            rule: Some(rule),
            holdback: false,
        }
    }

    pub fn holds(&self) -> bool {
        !self.send && !self.holdback
    }

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
            "held": !self.send,
            "holdback": self.holdback,
            "rule": self.rule,
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
        .chain(asked.noul(CRITERION_GOAL).map(|goal| 1.0 - goal))
        .fold(0.0_f64, f64::max);
    Some(Verdict {
        send: unfinished >= HOLD_BELOW && veto < VETO_AT,
        score: unfinished.min(1.0 - veto),
        rule: None,
        holdback: false,
    })
}
