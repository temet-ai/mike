use serde_json::{Value, json};

use super::questions::key;
use crate::llm::decisions::text::head_chars;

const PROMPT_CHARS: usize = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    RegexMatch,
    ShortSingleTurn,
}

impl Reason {
    pub fn code(self) -> &'static str {
        match self {
            Self::RegexMatch => "regex_match",
            Self::ShortSingleTurn => "short_single_turn",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Candidate {
    pub session_id: i64,
    pub first_prompt: String,
    pub project_path: Option<String>,
    pub reason: Reason,
}

pub fn state(group: &[&Candidate]) -> Value {
    let sessions: serde_json::Map<String, Value> = group
        .iter()
        .map(|candidate| {
            (
                key(candidate.session_id),
                json!({
                    "first_prompt": head_chars(&candidate.first_prompt, PROMPT_CHARS),
                    "project_path": candidate.project_path,
                    "probe_pattern_matched": candidate.reason == Reason::RegexMatch,
                }),
            )
        })
        .collect();
    json!({ "sessions": sessions })
}
