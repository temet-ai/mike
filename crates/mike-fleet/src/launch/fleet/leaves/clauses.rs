use crate::llm::decisions::Question;
use crate::llm::decisions::point::{Asked, noul};

use super::judge::CriterionText;

pub const MAX_CLAUSES: usize = 8;
pub const CLAUSE_SHOWN_AT: f64 = 0.7;

pub fn clause_key(criterion: &CriterionText, index: usize) -> String {
    format!("clause_{}_{index}", criterion.id)
}

pub fn clause_question(criterion: &CriterionText, clause: &str) -> Question {
    noul(
        &format!(
            "Criterion {} asks for this part: \"{clause}\". Does a test case named in `report.check_tests`, a line of `report.exercised_output`, or a line of `diff` show this part done?",
            criterion.id
        ),
        "A named test case, an output line or a diff line covers this part, such as a test named for it or a heading or line that states it.",
        "Nothing in the test names, the output or the diff covers this part.",
    )
}

pub fn clause_questions(criterion: &CriterionText) -> Vec<(String, Question)> {
    clauses(&criterion.text)
        .iter()
        .enumerate()
        .map(|(i, clause)| (clause_key(criterion, i), clause_question(criterion, clause)))
        .collect()
}

pub fn every_clause_shown(asked: &Asked, criterion: &CriterionText) -> bool {
    let count = clauses(&criterion.text).len();
    count > 0
        && (0..count).all(|i| {
            asked
                .noul(&clause_key(criterion, i))
                .is_some_and(|p| p >= CLAUSE_SHOWN_AT)
        })
}
const MIN_CLAUSE_CHARS: usize = 3;

fn last_group(text: &str) -> Option<&str> {
    let open = text.rfind('(')?;
    let close = text[open..].find(')')? + open;
    let inner = &text[open + 1..close];
    (inner.contains(':') || inner.contains(';')).then_some(inner)
}

fn after_last_colon(text: &str) -> &str {
    let tail = text.rfind(": ").map_or(text, |at| &text[at + 2..]);
    tail.find(". ").map_or(tail, |at| &tail[..at])
}

fn without_label(body: &str) -> &str {
    match body.find(": ") {
        Some(at) if !body[..at].contains(';') => &body[at + 2..],
        _ => body,
    }
}

fn split_list(body: &str) -> Vec<String> {
    let by_semicolon: Vec<&str> = body.split(';').collect();
    let pieces: Vec<String> = if by_semicolon.len() > 1 {
        by_semicolon.into_iter().map(str::to_owned).collect()
    } else {
        body.split(", ")
            .flat_map(|p| p.split(" and "))
            .map(str::to_owned)
            .collect()
    };
    pieces
        .into_iter()
        .map(|p| p.trim().trim_end_matches('.').trim().to_owned())
        .filter(|p| p.chars().count() >= MIN_CLAUSE_CHARS)
        .collect()
}

pub fn clauses(text: &str) -> Vec<String> {
    let body = last_group(text).map_or_else(|| after_last_colon(text), without_label);
    let mut out = split_list(body);
    if out.len() < 2 {
        return Vec::new();
    }
    out.truncate(MAX_CLAUSES);
    out
}
