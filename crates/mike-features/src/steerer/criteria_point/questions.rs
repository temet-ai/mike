use std::collections::BTreeMap;

use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "criteria_contract";
pub const QUESTION_SET: &str = "criteria_contract_v2";

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
                        "Does criteria.{} describe an observable end state: something that is true of the finished work and that a person could check? A finished thing stated in the past tense (has been mailed, is filed, was exercised end to end) or as a present state counts as yes.",
                        key(i)
                    ),
                    "The text says what is true or observable once the work is done, including a finished thing in the past tense or as a present state.",
                    "The text is an instruction (add, run, make sure) naming nothing that is true afterwards.",
                ),
            )
        })
        .collect()
}
