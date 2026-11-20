use std::collections::BTreeMap;

use super::rules::Rule;
use crate::llm::decisions::Question;
use crate::llm::decisions::point::noul;

pub const POINT: &str = "rule_check";
pub const QUESTION_SET: &str = "rule_check_v4";

pub fn key(index: usize) -> String {
    format!("rule_{index}")
}

pub fn requires_key(index: usize) -> String {
    format!("rule_{index}_requires")
}

pub fn applies_key(index: usize) -> String {
    format!("rule_{index}_applies")
}

fn applies(index: usize) -> Question {
    noul(
        &format!(
            "Does a call in `tool_calls` do the kind of work `rules.{}` is about?",
            key(index)
        ),
        "A call does that kind of work, e.g. the rule is about committing and a call runs git commit, or the rule is about database queries and a call runs psql.",
        "No call does that kind of work, e.g. the rule is about committing and nothing is committed, the rule is setup advice and nothing is set up, or the rule describes how a system works.",
    )
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

pub fn questions(rules: &[&Rule], committed: bool) -> BTreeMap<String, Question> {
    let mut asked = BTreeMap::new();
    for (index, rule) in rules.iter().enumerate() {
        asked.insert(key(index), forbids(index));
        asked.insert(applies_key(index), applies(index));
        if rule.is_obligation() && (committed || !rule.is_about_commits()) {
            asked.insert(requires_key(index), leaves_out(index));
        }
    }
    asked
}
