pub mod cache;
pub mod compose;
pub mod questions;

#[cfg(test)]
mod tests;

use std::time::Instant;

use serde_json::{Value, json};

pub use compose::{PROD_AT, Verdict, compose};
pub use questions::{POINT, QUESTION_SET};

use crate::llm::decisions::point::Decider;
use crate::llm::decisions::record::DecisionSubject;
use crate::llm::decisions::text::{head_chars, tail_chars};
use crate::steerer::guard::label_rules::is_prod;

const MESSAGE_CHARS: usize = 600;
const REPLY_TAIL_CHARS: usize = 300;

pub fn reply_tail(previous_reply: Option<&str>) -> Option<&str> {
    let reply = previous_reply?.trim();
    (!reply.is_empty()).then(|| tail_chars(reply, REPLY_TAIL_CHARS))
}

pub fn state(message: &str, previous_reply: Option<&str>) -> Value {
    json!({
        "message": head_chars(message.trim(), MESSAGE_CHARS),
        "message_chars": message.trim().chars().count(),
        "previous_reply_tail": reply_tail(previous_reply),
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Detected {
    pub is_prod: bool,
    pub failed: bool,
}

pub async fn detect(
    decider: Option<&Decider>,
    message: &str,
    previous_reply: Option<&str>,
    subject: DecisionSubject,
) -> bool {
    detect_checked(decider, message, previous_reply, subject)
        .await
        .is_prod
}

pub async fn detect_checked(
    decider: Option<&Decider>,
    message: &str,
    previous_reply: Option<&str>,
    subject: DecisionSubject,
) -> Detected {
    let regex_answer = is_prod(message);
    let off = Detected {
        is_prod: regex_answer,
        failed: false,
    };
    let Some(decider) = decider else {
        return off;
    };
    if message.trim().is_empty() {
        return off;
    }
    let key = cache::NudgeCache::key(message, reply_tail(previous_reply));
    if let Some(is_prod) = cache::SHARED.get(key, Instant::now()) {
        return Detected {
            is_prod,
            failed: false,
        };
    }
    let asked = decider
        .ask(
            QUESTION_SET,
            state(message, previous_reply),
            questions::questions(),
            subject,
            json!({ "prod_at": PROD_AT }),
        )
        .await;
    let verdict = compose(&asked);
    let used = verdict.filter(|_| decider.applies());
    let composed = json!({
        "score": verdict.map_or(0.0, |v| v.score()),
        "regex_said_prod": regex_answer,
    });
    let applied = if used.is_some() { "chosen" } else { "none" };
    decider
        .mark(&asked, asked.composed(composed), applied)
        .await;
    if let Some(v) = used {
        cache::SHARED.put(key, v.is_prod, Instant::now());
    }
    Detected {
        is_prod: used.map_or(regex_answer, |v| v.is_prod),
        failed: asked.failed(),
    }
}
