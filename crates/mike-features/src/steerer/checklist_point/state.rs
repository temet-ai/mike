use serde_json::{Map, Value, json};

use super::facts::ends_with_operator_question;
use super::questions::key;
use crate::llm::decisions::text::tail_chars;
use crate::store::persistence::acceptance_criteria::AcceptanceCriteriaMap;

pub const MAX_CRITERIA: usize = 15;
const CRITERION_CHARS: usize = 400;
const DIGEST_CHARS: usize = 3_500;
const TURN_END_CHARS: usize = 1_200;
const RULE_OPENINGS: &[&str] = &[
    "no ",
    "nothing ",
    "never ",
    "do not ",
    "don't ",
    "don\u{2019}t ",
    "without ",
];

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Criterion {
    pub text: String,
    pub inferred: bool,
    pub context: Option<String>,
}

impl Criterion {
    pub fn is_rule(&self) -> bool {
        is_rule(&self.text)
    }
}

pub fn is_rule(text: &str) -> bool {
    let lower = text.trim_start().to_lowercase();
    RULE_OPENINGS
        .iter()
        .any(|opening| lower.starts_with(opening))
}

pub struct Evidence<'a> {
    pub trajectory: &'a str,
    pub turn_end: &'a str,
    pub earlier_turns: &'a [String],
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
    let text = text.strip_prefix("[operator asked]").unwrap_or(text).trim();
    (!text.is_empty()).then(|| Criterion {
        text: text.to_owned(),
        inferred,
        context: None,
    })
}

fn has_marker(line: &str) -> bool {
    strip_marker(line).len() != line.trim().len()
}

fn their_words(line: &str) -> Option<&str> {
    let rest = line.trim().strip_prefix("(their words:")?.trim();
    let rest = rest.strip_suffix(')').unwrap_or(rest).trim();
    Some(rest.trim_matches(['"', '\u{201c}', '\u{201d}']).trim())
}

fn is_heading(line: &str) -> bool {
    let text = line.trim();
    text.ends_with(':') || text.to_lowercase().starts_with("acceptance criteria")
}

fn continues_criterion(line: &str) -> bool {
    line.starts_with([' ', '\t'])
}

fn absorb(found: &mut Vec<Criterion>, line: &str, listed: bool) {
    if let Some(words) = their_words(line) {
        if let Some(last) = found.last_mut() {
            last.context = (!words.is_empty()).then(|| words.to_owned());
        }
        return;
    }
    if line.trim().is_empty() || is_heading(line) {
        return;
    }
    if !listed || has_marker(line) {
        found.extend(parse_line(line));
        return;
    }
    if let Some(last) = found.last_mut().filter(|_| continues_criterion(line)) {
        last.text.push(' ');
        last.text.push_str(line.trim());
    }
}

pub fn thread_criteria(block: &str) -> Vec<Criterion> {
    let listed = block.lines().any(has_marker);
    let mut found = Vec::new();
    for line in block.lines() {
        absorb(&mut found, line, listed);
    }
    found.truncate(MAX_CRITERIA);
    found
}

pub fn intake_criteria(map: &AcceptanceCriteriaMap) -> Vec<Criterion> {
    map.criteria
        .iter()
        .filter(|row| !row.criterion.trim().is_empty() && !is_heading(&row.criterion))
        .map(|row| Criterion {
            text: row.criterion.trim().to_owned(),
            inferred: row.provenance != "operator",
            context: row
                .operator_span
                .as_deref()
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_owned),
        })
        .take(MAX_CRITERIA)
        .collect()
}

fn clip(text: &str) -> String {
    if text.chars().count() > CRITERION_CHARS {
        tail_chars(text, CRITERION_CHARS).to_owned()
    } else {
        text.to_owned()
    }
}

fn criterion_value(criterion: &Criterion) -> Value {
    let kind = if criterion.is_rule() {
        "rule"
    } else {
        "outcome"
    };
    let mut value = json!({ "text": clip(&criterion.text), "kind": kind });
    if let Some(words) = &criterion.context {
        value["their_words"] = Value::String(clip(words));
    }
    value
}

pub fn tail_lines(text: &str, max: usize) -> String {
    let mut used = 0;
    let kept: Vec<&str> = text
        .lines()
        .rev()
        .take_while(|line| {
            used += line.chars().count() + 1;
            used <= max + 1
        })
        .collect();
    if kept.is_empty() {
        return tail_chars(text, max).to_owned();
    }
    kept.into_iter().rev().collect::<Vec<_>>().join("\n")
}

pub fn state(criteria: &[Criterion], evidence: &Evidence<'_>) -> Value {
    let by_key: Map<String, Value> = criteria
        .iter()
        .enumerate()
        .map(|(i, c)| (key(i), criterion_value(c)))
        .collect();
    json!({
        "criteria": by_key,
        "evidence": {
            "trajectory": tail_lines(evidence.trajectory.trim_end(), DIGEST_CHARS),
            "turn_end": tail_chars(evidence.turn_end.trim_end(), TURN_END_CHARS),
        },
        "facts": {
            "ends_with_operator_question": ends_with_operator_question(evidence.turn_end),
            "earlier_turns": evidence.earlier_turns,
        },
    })
}
