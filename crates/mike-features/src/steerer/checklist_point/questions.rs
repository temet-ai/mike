use std::collections::BTreeMap;

use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "steerer_checklist";
pub const QUESTION_SET: &str = "steerer_checklist_v1";

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
                        "Does state.evidence (the tool calls of this turn and the end of the turn) show that criteria.{} is done?",
                        key(i)
                    ),
                    "The evidence shows this criterion satisfied: a tool call or the turn's own text covers it.",
                    "The evidence does not show this criterion satisfied, or shows it left undone.",
                ),
            )
        })
        .collect()
}
