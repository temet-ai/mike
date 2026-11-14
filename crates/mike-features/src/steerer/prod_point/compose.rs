use super::questions::ONLY_NUDGE;
use crate::llm::decisions::point::Asked;

pub const PROD_AT: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Verdict {
    pub is_prod: bool,
    pub probability: f64,
}

impl Verdict {
    pub fn score(&self) -> f64 {
        if self.is_prod {
            self.probability
        } else {
            1.0 - self.probability
        }
    }
}

pub fn compose(asked: &Asked) -> Option<Verdict> {
    if asked.failed() {
        return None;
    }
    let probability = asked.noul(ONLY_NUDGE)?;
    Some(Verdict {
        is_prod: probability >= PROD_AT,
        probability,
    })
}
