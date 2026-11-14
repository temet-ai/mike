use serde_json::{Map, Value, json};

use super::questions::key;
use crate::llm::decisions::text::tail_chars;
use crate::store::persistence::acceptance_criteria::AcceptanceCriteriaMap;

pub const MAX_CRITERIA: usize = 15;
const CRITERION_CHARS: usize = 400;
const DIGEST_CHARS: usize = 3_500;
const TURN_END_CHARS: usize = 1_200;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Criterion {
    pub text: String,
    pub inferred: bool,
}

pub struct Evidence<'a> {
    pub trajectory: &'a str,
    pub turn_end: &'a str,
}

fn strip_marker(line: &str) -> &str {
    let line = line.trim();
    for marker in ["- ", "* ", "+ "] {
        if let Some(rest) = line.strip_prefix(marker) {
            return rest.trim_start();
        }
    }
    let digits = line.chars().take_while(char::is_ascii_digit).count();
    match line[digits..].strip_prefix(". ") {
        Some(rest) if digits > 0 => rest.trim_start(),
        _ => line,
    }
}

fn parse_line(line: &str) -> Option<Criterion> {
    let text = strip_marker(line);
    let (inferred, text) = match text.strip_prefix("[inferred") {
        Some(rest) => (true, rest.split_once(']').map_or(rest, |(_, after)| after)),
        None => (false, text),
    };
    let text = text.trim();
    (!text.is_empty()).then(|| Criterion {
        text: text.to_owned(),
        inferred,
    })
}

fn has_marker(line: &str) -> bool {
    strip_marker(line).len() != line.trim().len()
}

pub fn thread_criteria(block: &str) -> Vec<Criterion> {
    let listed = block.lines().any(has_marker);
    block
        .lines()
        .filter(|line| !listed || has_marker(line))
        .filter_map(parse_line)
        .take(MAX_CRITERIA)
        .collect()
}

pub fn intake_criteria(map: &AcceptanceCriteriaMap) -> Vec<Criterion> {
    map.criteria
        .iter()
        .filter(|row| !row.criterion.trim().is_empty())
        .map(|row| Criterion {
            text: row.criterion.trim().to_owned(),
            inferred: row.provenance != "operator",
        })
        .take(MAX_CRITERIA)
        .collect()
}

pub fn session_criteria(
    linked_thread: Option<&str>,
    intake: Option<&AcceptanceCriteriaMap>,
) -> Vec<Criterion> {
    match (linked_thread, intake) {
        (Some(block), _) => thread_criteria(block),
        (None, Some(map)) => intake_criteria(map),
        (None, None) => Vec::new(),
    }
}

pub fn state(criteria: &[Criterion], evidence: &Evidence<'_>) -> Value {
    let by_key: Map<String, Value> = criteria
        .iter()
        .enumerate()
        .map(|(i, c)| {
            let text = if c.text.chars().count() > CRITERION_CHARS {
                tail_chars(&c.text, CRITERION_CHARS).to_owned()
            } else {
                c.text.clone()
            };
            (key(i), Value::String(text))
        })
        .collect();
    json!({
        "criteria": by_key,
        "evidence": {
            "trajectory": tail_chars(evidence.trajectory.trim_end(), DIGEST_CHARS),
            "turn_end": tail_chars(evidence.turn_end.trim_end(), TURN_END_CHARS),
        },
    })
}
