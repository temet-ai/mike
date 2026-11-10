use serde_json::Value;
use uuid::Uuid;

use super::block::{Block, KEEPER_SOURCE};
use crate::compress::manifest::sha256_hex;

pub(crate) const EARLIER_MARKER_OPEN: &str = "[keeper-earlier:";
pub(crate) const MAX_SESSIONS: usize = 3;
const MAX_TERMS: usize = 12;
const EXCERPT_CHARS: usize = 160;
const HASH_CHARS: usize = 12;
const SUPPLIED_KINDS: &[&str] = &["turn", "thread_entry", "note"];

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

const STOP_WORDS: &[&str] = &[
    "the",
    "and",
    "but",
    "then",
    "that",
    "this",
    "these",
    "those",
    "there",
    "here",
    "with",
    "from",
    "into",
    "onto",
    "about",
    "what",
    "which",
    "who",
    "how",
    "why",
    "when",
    "where",
    "can",
    "could",
    "would",
    "should",
    "will",
    "may",
    "might",
    "must",
    "does",
    "did",
    "done",
    "have",
    "has",
    "had",
    "not",
    "yes",
    "just",
    "like",
    "also",
    "again",
    "yesterday",
    "last",
    "week",
    "weeks",
    "month",
    "months",
    "ago",
    "earlier",
    "before",
    "remember",
    "thought",
    "think",
    "fixed",
    "discussed",
    "decided",
    "didnt",
    "didn't",
    "we've",
    "were",
    "we're",
    "i'm",
    "it's",
    "its",
    "lets",
    "let's",
    "please",
    "check",
    "see",
    "find",
    "look",
    "session",
    "sessions",
    "mike",
    "mcp",
    "work",
    "worked",
    "working",
    "our",
    "some",
    "any",
    "all",
    "out",
    "now",
    "still",
    "really",
    "actually",
    "probably",
    "need",
    "want",
    "get",
    "got",
    "make",
    "made",
    "same",
    "thing",
    "things",
    "other",
    "more",
    "much",
    "very",
    "sure",
    "kind",
    "bit",
    "one",
    "two",
    "few",
    "lot",
    "lots",
    "around",
    "regarding",
    "status",
    "you",
    "your",
    "they",
    "them",
    "she",
    "him",
    "her",
    "was",
    "are",
    "been",
    "being",
    "for",
    "had",
    "able",
    "way",
    "there's",
    "that's",
    "what's",
    "gonna",
    "yeah",
];

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
    let lower = prompt.to_lowercase();
    PAST_PHRASES
        .iter()
        .any(|phrase| contains_words(&lower, phrase))
}

fn contains_words(text: &str, phrase: &str) -> bool {
    text.match_indices(phrase).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + phrase.len()..].chars().next();
        !before.is_some_and(char::is_alphanumeric) && !after.is_some_and(char::is_alphanumeric)
    })
}

pub(crate) fn search_query(prompt: &str) -> Option<String> {
    let mut terms: Vec<String> = Vec::new();
    for word in prompt.split_whitespace() {
        let word = word.trim_matches(|c: char| !c.is_alphanumeric());
        let lower = word.to_lowercase();
        if lower.chars().count() < 3 || STOP_WORDS.contains(&lower.as_str()) {
            continue;
        }
        if lower.starts_with("http") || is_long_number(&lower) {
            continue;
        }
        if !terms.contains(&lower) {
            terms.push(lower);
        }
    }
    terms.sort_by_key(|term| !is_identifier(term));
    terms.truncate(MAX_TERMS);
    (!terms.is_empty()).then(|| terms.join(" "))
}

fn is_identifier(term: &str) -> bool {
    term.contains(|c: char| c.is_ascii_digit()) || term.contains(['-', '_', '.'])
}

fn is_long_number(term: &str) -> bool {
    term.len() > 6 && term.chars().all(|c| c.is_ascii_digit())
}

pub(crate) fn hits_from(page: &Value) -> Vec<Hit> {
    page.get("items")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(hit_from).collect())
        .unwrap_or_default()
}

fn hit_from(item: &Value) -> Option<Hit> {
    let kind = item.get("source_kind")?.as_str()?;
    if !SUPPLIED_KINDS.contains(&kind) {
        return None;
    }
    let author = item.get("author");
    let session = author
        .and_then(|a| a.get("session_uuid"))
        .or_else(|| item.pointer("/provenance/session_uuid"))
        .and_then(Value::as_str)
        .and_then(|s| Uuid::parse_str(s).ok())?;
    let text = item
        .get("excerpt")
        .or_else(|| item.get("title"))
        .and_then(Value::as_str)
        .unwrap_or_default();
    Some(Hit {
        reference: item.get("reference")?.as_str()?.to_owned(),
        session,
        session_title: author
            .and_then(|a| a.get("session_title"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_owned(),
        date: item
            .get("updated_at")
            .and_then(Value::as_str)
            .map(|at| at.chars().take(10).collect())
            .unwrap_or_default(),
        excerpt: flatten(text, EXCERPT_CHARS),
        score: score_of(item),
    })
}

fn score_of(item: &Value) -> f64 {
    item.get("rerank_score")
        .and_then(Value::as_f64)
        .or_else(|| item.get("rank").and_then(Value::as_f64))
        .unwrap_or(f64::MIN)
}

fn flatten(text: &str, max: usize) -> String {
    let flat = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if flat.chars().count() <= max {
        return flat;
    }
    let cut: String = flat.chars().take(max - 1).collect();
    format!("{}…", cut.trim_end())
}

pub(crate) fn pick(mut hits: Vec<Hit>, own_session: Uuid) -> Vec<Hit> {
    hits.retain(|hit| hit.session != own_session);
    hits.sort_by(|a, b| {
        b.score
            .total_cmp(&a.score)
            .then_with(|| a.reference.cmp(&b.reference))
    });
    let mut picked: Vec<Hit> = Vec::new();
    for hit in hits {
        if picked.len() == MAX_SESSIONS {
            break;
        }
        if picked.iter().all(|p| p.session != hit.session) {
            picked.push(hit);
        }
    }
    picked
}

pub(crate) fn render(picked: &[Hit]) -> Option<Block> {
    if picked.is_empty() {
        return None;
    }
    let mut lines =
        vec!["Earlier sessions this prompt may point at, found by searching past work:".to_owned()];
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
