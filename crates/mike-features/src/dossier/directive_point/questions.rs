use std::collections::BTreeMap;

use super::state::Unit;
use crate::llm::decisions::Question;
use crate::llm::decisions::point::choice;

pub const POINT: &str = "directive";
pub const QUESTION_SET: &str = "directive_v1";

pub const OPTIONS: &[(&str, &str)] = &[
    (
        "authorises_now",
        "The operator tells the session it may do this action now or from here on.",
    ),
    (
        "forbids",
        "The operator tells the session not to do this action, or to ask before doing it.",
    ),
    (
        "neither",
        "The sentence mentions the action without granting or forbidding it: a request about something else, a question, a quote, or a different meaning of the word such as push back.",
    ),
];

pub fn key(index: usize) -> String {
    format!("unit_{index}")
}

pub fn questions(group: &[&Unit]) -> BTreeMap<String, Question> {
    group
        .iter()
        .enumerate()
        .map(|(i, _)| {
            (
                key(i),
                choice(
                    &format!(
                        "Read units.{k}.sentence inside units.{k}.turn. Does the operator authorise now, forbid, or do neither for the action class units.{k}.action_class? The classes are commit, push, merge, deploy and pr; proceed means go ahead without asking; ask_first means ask the operator before acting.",
                        k = key(i)
                    ),
                    OPTIONS,
                ),
            )
        })
        .collect()
}
