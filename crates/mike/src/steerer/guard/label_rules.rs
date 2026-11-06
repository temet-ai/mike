use std::sync::LazyLock;

use regex::Regex;
use serde_json::{Value, json};

use super::Question;

pub const GUARD_LABELLER_VERSION: &str = "guard-labeller-v1";
pub const PROD_MAX_CHARS: usize = 20;

static PROD_LIST_V1: LazyLock<Option<Regex>> = LazyLock::new(|| {
    Regex::new(r"(?i)^(continue|go on|go ahead|carry on|proceed|keep going|\?+|status\??|any update\??|and\??|well\??|ping|still there\??|hello\??)[.!?]*$").ok()
});

pub fn is_prod(message: &str) -> bool {
    let trimmed = message.trim();
    trimmed.chars().count() <= PROD_MAX_CHARS
        || PROD_LIST_V1.as_ref().is_some_and(|re| re.is_match(trimmed))
}

#[derive(Debug, Clone, PartialEq)]
pub struct LabelInput<'a> {
    pub question: Question,
    pub would_fire: bool,
    pub actor_primary: bool,
    pub reaction: Option<&'a str>,
    pub session_end: Option<&'a str>,
    pub exempt: Option<&'a str>,
    pub superseded_by: Option<uuid::Uuid>,
    pub has_next_message: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Settlement {
    Label {
        label: &'static str,
        source: &'static str,
        evidence: Value,
    },
    NeedsCall,
    Wait,
}

fn label(label: &'static str, source: &'static str, evidence: Value) -> Settlement {
    Settlement::Label {
        label,
        source,
        evidence,
    }
}

fn fire_pass(would_fire: bool, fire: &'static str, pass: &'static str) -> &'static str {
    if would_fire { fire } else { pass }
}

fn by_session_end(i: &LabelInput<'_>, end: &str) -> Settlement {
    let stall = matches!(i.question, Question::Relapse | Question::UnbackedWait);
    let evidence = json!({"session_end_label": end});
    match end {
        "done" if stall => label(
            fire_pass(i.would_fire, "wrong", "right"),
            "session_end",
            evidence,
        ),
        "done" => label("indeterminate", "session_end", evidence),
        "closed_in_frustration" if stall => label(
            fire_pass(i.would_fire, "right", "wrong"),
            "session_end",
            evidence,
        ),
        "closed_in_frustration" => Settlement::NeedsCall,
        _ => label("indeterminate", "session_end", evidence),
    }
}

fn by_reaction(i: &LabelInput<'_>, reaction: &str) -> Option<Settlement> {
    let evidence = json!({"reaction": reaction, "source": "live_verdict"});
    match reaction {
        "unclear" => Some(label("indeterminate", "next_message", evidence)),
        "accepted" | "redirected" => Some(match i.question {
            Question::UnobservedDone | Question::AuthorisedHandback => label(
                fire_pass(i.would_fire, "wrong", "right"),
                "next_message",
                evidence,
            ),
            Question::Relapse | Question::UnbackedWait => Settlement::NeedsCall,
        }),
        "corrected" => Some(Settlement::NeedsCall),
        _ => None,
    }
}

pub fn settle(i: &LabelInput<'_>) -> Settlement {
    if let Some(by) = i.superseded_by {
        return label(
            "indeterminate",
            "next_message",
            json!({"superseded_by": by}),
        );
    }
    if let Some(reason) = i.exempt {
        return label("indeterminate", "next_message", json!({"exempt": reason}));
    }
    if i.would_fire && !i.actor_primary {
        return label("wrong", "next_message", json!({"actor": "not_primary"}));
    }
    if let Some(s) = i.reaction.and_then(|r| by_reaction(i, r)) {
        return s;
    }
    if let Some(end) = i.session_end {
        return by_session_end(i, end);
    }
    if i.has_next_message && i.reaction.is_none() {
        Settlement::NeedsCall
    } else {
        Settlement::Wait
    }
}

pub fn from_answer(would_fire: bool, answer: &str) -> &'static str {
    match (would_fire, answer) {
        (true, "yes") | (false, "no") => "right",
        (true, "no") | (false, "yes") => "wrong",
        _ => "indeterminate",
    }
}
