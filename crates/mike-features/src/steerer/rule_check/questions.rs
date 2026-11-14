use std::collections::BTreeMap;

use super::rules::Rule;
use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "rule_check";
pub const QUESTION_SET: &str = "rule_check_v1";

pub fn key(index: usize) -> String {
    format!("rule_{index}")
}

pub fn questions(rules: &[&Rule]) -> BTreeMap<String, Question> {
    rules
        .iter()
        .enumerate()
        .map(|(i, _)| {
            (
                key(i),
                noul(
                    &format!(
                        "Do the tool_calls or files_written of this turn break the rule at rules.{}: do they do what it forbids or leave out what it requires?",
                        key(i)
                    ),
                    "The tool calls or files show the rule being broken in this turn.",
                    "Nothing in the tool calls or files breaks the rule, or the rule does not apply to what was done.",
                ),
            )
        })
        .collect()
}
