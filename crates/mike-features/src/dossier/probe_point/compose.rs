use super::questions::key;
use super::state::Candidate;
use crate::llm::decisions::point::Asked;

pub const PROBE_AT: f64 = 0.5;

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Verdict {
    pub session_id: i64,
    pub is_probe: bool,
    pub probability: f64,
}

impl Verdict {
    pub fn score(&self) -> f64 {
        if self.is_probe {
            self.probability
        } else {
            1.0 - self.probability
        }
    }
}

pub fn compose(asked: &Asked, group: &[&Candidate]) -> Vec<Verdict> {
    if asked.failed() {
        return Vec::new();
    }
    group
        .iter()
        .filter_map(|candidate| {
            let probability = asked.noul(&key(candidate.session_id))?;
            Some(Verdict {
                session_id: candidate.session_id,
                is_probe: probability >= PROBE_AT,
                probability,
            })
        })
        .collect()
}
