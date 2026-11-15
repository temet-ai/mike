use serde_json::{Value, json};

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
}

fn follows(input: &Unmatched) -> (&'static str, &str) {
    let previous = input.previous_user_message.trim();
    let repeats = previous == input.text.trim() || previous == input.last_user_message.trim();
    if previous.is_empty() || repeats {
        (
            "previous_assistant_reply",
            tail_chars(input.previous_assistant_reply.trim(), REPLY_CHARS),
        )
    } else {
        ("previous_user_message", tail_chars(previous, FOLLOWS_CHARS))
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
        "text_chars": total,
        "follows": follows_text,
        "follows_is": follows_kind,
        "author_guess": input.author,
        "sent_by_subagent": input.has_agent_id,
    })
}
