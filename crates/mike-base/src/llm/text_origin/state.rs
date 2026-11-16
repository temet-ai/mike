use serde_json::{Value, json};

use super::signals::Signals;
use crate::llm::decisions::operator_text::typed_by_operator;
use crate::llm::decisions::text::{head_chars, tail_chars};

const TEXT_HEAD_CHARS: usize = 900;
const TEXT_TAIL_CHARS: usize = 300;
const FOLLOWS_CHARS: usize = 400;
const REPLY_CHARS: usize = 300;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unmatched {
    pub request_id: i64,
    pub session_db_id: i64,
    pub text: String,
    pub last_user_message: String,
    pub previous_user_message: String,
    pub previous_assistant_reply: String,
    pub author: String,
    pub has_agent_id: bool,
    pub signals: Signals,
}

fn sent_before(count: usize) -> String {
    match count {
        0 => "no earlier user message in this session has this text".to_owned(),
        1 => "one earlier user message in this session has exactly this text".to_owned(),
        n => format!("{n} earlier user messages in this session have exactly this text"),
    }
}

fn client(entrypoint: Option<&str>) -> &'static str {
    match entrypoint {
        Some(e) if e.starts_with("sdk") => {
            "a program started this session as a non-interactive run (claude -p or the SDK); nobody types into it"
        }
        Some(_) => "an interactive Claude Code session a person can type into",
        None => "not known",
    }
}

fn facts(signals: &Signals) -> Value {
    json!({
        "sent_before": sent_before(signals.sent_before),
        "scheduled_prompt": signals.scheduled_by.as_deref().map_or_else(
            || "no scheduler in this session was given this text".to_owned(),
            |tool| format!("this session set up {tool} with exactly this text as its prompt"),
        ),
        "client": client(signals.entrypoint.as_deref()),
    })
}

fn follows(input: &Unmatched) -> (&'static str, String) {
    let previous = input.previous_user_message.trim();
    let repeats = previous == input.text.trim() || previous == input.last_user_message.trim();
    let typed = if previous.is_empty() || repeats {
        None
    } else {
        typed_by_operator(previous)
    };
    match typed {
        Some(typed) => (
            "previous_user_message",
            tail_chars(typed.trim(), FOLLOWS_CHARS).to_owned(),
        ),
        None => (
            "previous_assistant_reply",
            tail_chars(input.previous_assistant_reply.trim(), REPLY_CHARS).to_owned(),
        ),
    }
}

pub fn state(input: &Unmatched) -> Value {
    let (follows_kind, follows_text) = follows(input);
    let total = input.text.chars().count();
    let text = if total > TEXT_HEAD_CHARS + TEXT_TAIL_CHARS {
        format!(
            "{} [...] {}",
            head_chars(&input.text, TEXT_HEAD_CHARS),
            tail_chars(&input.text, TEXT_TAIL_CHARS)
        )
    } else {
        input.text.clone()
    };
    json!({
        "text": text,
        "follows": follows_text,
        "follows_is": follows_kind,
        "facts": facts(&input.signals),
    })
}
