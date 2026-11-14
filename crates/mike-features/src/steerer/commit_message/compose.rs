use serde_json::{Value, json};

use super::questions::{DESCRIBES, UNMENTIONED};
use super::state::Commit;
use crate::llm::decisions::point::Asked;

pub const DESCRIBES_BELOW: f64 = 0.4;
pub const UNMENTIONED_AT: f64 = 0.7;
const FILES_NAMED: usize = 5;

#[derive(Debug, Clone, PartialEq)]
pub struct Warning {
    pub warn: bool,
    pub mismatch: bool,
    pub describes: f64,
    pub unmentioned: f64,
}

impl Warning {
    pub fn score(&self) -> f64 {
        if self.warn {
            (1.0 - self.describes).max(self.unmentioned)
        } else {
            self.describes.min(1.0 - self.unmentioned)
        }
    }

    pub fn json(&self, delivered: bool) -> Value {
        json!({
            "score": self.score(),
            "warn": self.warn,
            "mismatch": self.mismatch,
            "describes": self.describes,
            "unmentioned": self.unmentioned,
            "describes_below": DESCRIBES_BELOW,
            "unmentioned_at": UNMENTIONED_AT,
            "delivered": delivered,
        })
    }

    pub fn note(&self, commit: &Commit) -> String {
        let short = commit.sha.get(..7).unwrap_or(&commit.sha);
        let files = commit
            .files
            .iter()
            .take(FILES_NAMED)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        if self.mismatch {
            format!(
                "The message of commit {short} (\"{}\") does not seem to describe what the commit changes ({files}). If the message is wrong, amend it before pushing; if it is right, say what in the diff it covers.",
                commit.subject
            )
        } else {
            format!(
                "Commit {short} (\"{}\") also changes work its message does not mention ({files}). If those changes belong in their own commit, split them; otherwise add them to the message.",
                commit.subject
            )
        }
    }
}

pub fn compose(asked: &Asked) -> Option<Warning> {
    if asked.failed() {
        return None;
    }
    let describes = asked.noul(DESCRIBES)?;
    let unmentioned = asked.noul(UNMENTIONED)?;
    let mismatch = describes < DESCRIBES_BELOW;
    Some(Warning {
        warn: mismatch || unmentioned >= UNMENTIONED_AT,
        mismatch,
        describes,
        unmentioned,
    })
}
