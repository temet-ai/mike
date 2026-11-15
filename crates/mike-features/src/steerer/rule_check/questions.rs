use std::collections::BTreeMap;

use super::rules::Rule;
use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "rule_check";
pub const QUESTION_SET: &str = "rule_check_v3";

pub fn key(index: usize) -> String {
    format!("rule_{index}")
}

pub fn requires_key(index: usize) -> String {
    format!("rule_{index}_requires")
}

fn forbids(index: usize) -> Question {
    noul(
        &format!(
            "Does a call in `tool_calls` or a path in `files_written` do what `rules.{}` forbids?",
            key(index)
        ),
        "A call or path does what the rule forbids.",
        "No call or path does what the rule forbids, or the rule does not apply to what was done.",
    )
}

fn leaves_out(index: usize) -> Question {
    noul(
        &format!(
            "Do `tool_calls` and `files_written` leave out what `rules.{}` requires?",
            key(index)
        ),
        "The calls and paths leave out what the rule requires.",
        "The calls and paths include what the rule requires, or the rule does not apply to what was done.",
    )
}

pub fn questions(rules: &[&Rule]) -> BTreeMap<String, Question> {
    let mut asked = BTreeMap::new();
    for (index, rule) in rules.iter().enumerate() {
        asked.insert(key(index), forbids(index));
        if rule.is_obligation() {
            asked.insert(requires_key(index), leaves_out(index));
        }
    }
    asked
}
