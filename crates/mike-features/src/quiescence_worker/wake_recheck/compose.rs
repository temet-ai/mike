use serde_json::{Value, json};

use super::questions::{DONE_SINCE, STILL_OPEN};
use crate::llm::decisions::point::Asked;

pub const NEEDED_BELOW: f64 = 0.35;

#[derive(Debug, Clone, PartialEq)]
pub struct Recheck {
    pub still_needed: f64,
    pub skip: bool,
}

impl Recheck {
    pub fn json(&self, pushed: bool) -> Value {
        let right = if self.skip {
            1.0 - self.still_needed
        } else {
            self.still_needed
        };
        json!({
            "score": right,
            "still_needed": self.still_needed,
            "skip": self.skip,
            "pushed": pushed,
            "needed_below": NEEDED_BELOW,
        })
    }
}

pub fn compose(asked: &Asked) -> Option<Recheck> {
    if asked.failed() {
        return None;
    }
    let open = asked.noul(STILL_OPEN)?;
    let done = asked.noul(DONE_SINCE)?;
    let still_needed = (open + (1.0 - done)) / 2.0;
    Some(Recheck {
        still_needed,
        skip: still_needed < NEEDED_BELOW,
    })
}
