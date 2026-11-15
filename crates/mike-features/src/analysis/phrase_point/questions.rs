use std::collections::BTreeMap;

use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "diagnostic_phrase";
pub const QUESTION_SET: &str = "diagnostic_phrase_v2";

pub fn key(index: usize) -> String {
    format!("match_{index}")
}

pub fn questions(count: usize) -> BTreeMap<String, Question> {
    (0..count)
        .map(|i| {
            (
                key(i),
                noul(
                    &format!(
                        "In matches.{k}.sentence, is the phrase matches.{k}.phrase used to excuse skipped, postponed or reduced work, or to deflect blame for it?",
                        k = key(i)
                    ),
                    "The sentence uses the phrase to justify leaving work undone, putting it off, shrinking its scope or blaming something else. Blaming a failure on a condition that was already there before the change is an excuse, even when it is stated as plain fact.",
                    "The phrase is used in an ordinary, descriptive way and excuses no skipped work. Quoting the operator's decision, or describing the current state of the code or system, is not an excuse.",
                ),
            )
        })
        .collect()
}
