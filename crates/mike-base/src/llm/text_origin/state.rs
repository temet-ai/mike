use serde_json::{Value, json};

use crate::llm::decisions::text::{head_chars, tail_chars};

const TEXT_HEAD_CHARS: usize = 900;
const TEXT_TAIL_CHARS: usize = 300;
const FOLLOWS_CHARS: usize = 400;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unmatched {
    pub request_id: i64,
    pub session_db_id: i64,
    pub text: String,
    pub last_user_message: String,
    pub author: String,
    pub has_agent_id: bool,
}

pub fn state(input: &Unmatched) -> Value {
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
        "follows": tail_chars(&input.last_user_message, FOLLOWS_CHARS),
        "author_guess": input.author,
        "sent_by_subagent": input.has_agent_id,
    })
}
