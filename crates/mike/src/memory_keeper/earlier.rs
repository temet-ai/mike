use std::ops::Range;

use chrono::Duration;
use uuid::Uuid;

use super::block::{Block, KEEPER_SOURCE};
use crate::compress::manifest::sha256_hex;

pub(crate) const EARLIER_MARKER_OPEN: &str = "[keeper-earlier:";
pub(crate) const MAX_SESSIONS: usize = 3;
const HASH_CHARS: usize = 12;

const PAST_PHRASES: &[&str] = &[
    "we did",
    "we had",
    "we made",
    "we worked",
    "we decided",
    "we discussed",
    "we agreed",
    "we fixed",
    "we found",
    "we tried",
    "we specified",
    "we discovered",
    "we released",
    "last week",
    "last time",
    "yesterday",
    "the other day",
    "earlier",
    "i thought we",
    "didn't we",
    "didnt we",
    "remember",
    "ago",
    "again",
    "there was a session",
    "that session",
    "previous session",
];

const YESTERDAY: &[&str] = &["yesterday"];
const LAST_WEEK: &[&str] = &["last week", "the other day", "few days ago"];
const LONG_AGO: &[&str] = &["month", "months", "weeks ago", "a while ago"];

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Hit {
    pub(crate) reference: String,
    pub(crate) session: Uuid,
    pub(crate) session_title: String,
    pub(crate) date: String,
    pub(crate) excerpt: String,
    pub(crate) score: f64,
}

pub(crate) fn operator_words(text: &str) -> String {
    const OPEN: &str = "<system-reminder";
    const CLOSE: &str = "</system-reminder>";
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(OPEN) {
        out.push_str(&rest[..start]);
        rest = match rest[start..].find(CLOSE) {
            Some(end) => &rest[start + end + CLOSE.len()..],
            None => "",
        };
    }
    out.push_str(rest);
    out
}

pub(crate) fn points_at_past(prompt: &str) -> bool {
    mentions_any(&prompt.to_lowercase(), PAST_PHRASES)
}

pub(crate) fn window(prompt: &str) -> Range<Duration> {
    let lower = prompt.to_lowercase();
    if mentions_any(&lower, YESTERDAY) {
        return Duration::minutes(30)..Duration::days(3);
    }
    if mentions_any(&lower, LAST_WEEK) {
        return Duration::days(1)..Duration::days(15);
    }
    if mentions_any(&lower, LONG_AGO) {
        return Duration::days(7)..Duration::days(120);
    }
    Duration::minutes(30)..Duration::days(30)
}

fn mentions_any(lower: &str, phrases: &[&str]) -> bool {
    phrases.iter().any(|phrase| contains_words(lower, phrase))
}

fn contains_words(text: &str, phrase: &str) -> bool {
    text.match_indices(phrase).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + phrase.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

pub(crate) fn flatten(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let cut: String = flat.chars().take(max - 1).collect();
    format!("{}…", cut.trim_end())
}

pub(crate) fn render(picked: &[Hit]) -> Option<Block> {
    if picked.is_empty() {
        return None;
    }
    let mut lines = vec![
        "Earlier sessions this prompt may point at, found by meaning among past turns:".to_owned(),
    ];
    for hit in picked {
        let session: String = hit.session.to_string().chars().take(8).collect();
        let title = if hit.session_title.is_empty() {
            String::new()
        } else {
            format!(" \"{}\"", flatten(&hit.session_title, 60))
        };
        lines.push(format!(
            "- {}, session {session}{title}: {} [{}]",
            hit.date, hit.excerpt, hit.reference
        ));
    }
    lines.push("Read one with read_project_knowledge <reference>.".to_owned());
    let body = lines.join("\n");
    let hash: String = sha256_hex(body.as_bytes())
        .chars()
        .take(HASH_CHARS)
        .collect();
    let text = format!(
        "<system-reminder source=\"{KEEPER_SOURCE}\">\n{EARLIER_MARKER_OPEN}{hash}]\n{body}\n</system-reminder>"
    );
    Some(Block { text, hash })
}

#[cfg(test)]
#[path = "earlier_tests.rs"]
mod tests;
