use std::collections::BTreeMap;

use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "criteria_source";
pub const QUESTION_SET: &str = "criteria_source_v2";

pub fn key(index: usize) -> String {
    format!("thread_{index}")
}

pub fn questions(count: usize) -> BTreeMap<String, Question> {
    (0..count)
        .map(|i| {
            (
                key(i),
                noul(
                    &format!(
                        "Reading everything in state.operator_asks, did the operator ask this session to do the work that state.threads.{} describes?",
                        key(i)
                    ),
                    "The operator asked this session for that thread's work, by naming the thread or by asking for what its criteria describe.",
                    "The operator asked this session for other work; the thread is linked only because the session read or wrote it.",
                ),
            )
        })
        .collect()
}
