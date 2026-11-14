use std::collections::BTreeMap;

use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "criteria_contract";
pub const QUESTION_SET: &str = "criteria_contract_v1";

pub fn key(index: usize) -> String {
    format!("criterion_{index}")
}

pub fn questions(count: usize) -> BTreeMap<String, Question> {
    (0..count)
        .map(|i| {
            (
                key(i),
                noul(
                    &format!(
                        "Does criteria.{} describe an observable end state: something that is true of the finished work and that a person could check?",
                        key(i)
                    ),
                    "The text says what is true or observable once the work is done.",
                    "The text commands work to be done, or names an activity, without saying what is true afterwards.",
                ),
            )
        })
        .collect()
}
