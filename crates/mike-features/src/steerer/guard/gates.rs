use regex::Regex;
use serde::Serialize;
use serde_json::{Value, json};

use super::step_class::StepClass;
use crate::dossier::build::ended_on::{DONE_VOCAB, HANDBACK, NEXT_STEP, WAIT};

pub const GATE_VERSION: &str = "gate-v1";

pub const ARMING_MARKERS: [&str; 15] = [
    "all tests pass",
    "tests are passing",
    "done.",
    "done!",
    "finished",
    "completed",
    "should be good now",
    "should be fixed now",
    "ready for review",
    "fixed the",
    "resolved the",
    "this is complete",
    "the fix is",
    "i've verified",
    "i have verified",
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EndedOn {
    Handback,
    Question,
    PromiseOrWait,
    Report,
    Other,
}

impl EndedOn {
    pub fn code(self) -> &'static str {
        match self {
            Self::Handback => "handback",
            Self::Question => "question",
            Self::PromiseOrWait => "promise_or_wait",
            Self::Report => "report",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct PhraseGates {
    pub g2: bool,
    pub g3: bool,
    pub g4: bool,
    pub g4b: bool,
    pub ended_on: EndedOn,
    pub empty: bool,
    pub handback_match: Option<String>,
    pub wait_match: Option<String>,
    pub done_match: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub step_class: Option<StepClass>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct GateOutcome {
    pub open: bool,
    pub gate_id: String,
    pub arm: Option<&'static str>,
    pub reason: String,
    pub matched_text: Option<String>,
    pub facts_used: Value,
    pub other_arms: Value,
}

impl GateOutcome {
    pub fn open(gate: &str, matched_text: Option<String>, facts_used: Value) -> Self {
        Self {
            open: true,
            gate_id: format!("{GATE_VERSION}:{gate}"),
            arm: None,
            reason: "open".to_owned(),
            matched_text,
            facts_used,
            other_arms: json!({}),
        }
    }

    pub fn closed(gate: &str, reason: impl Into<String>, facts_used: Value) -> Self {
        Self {
            open: false,
            gate_id: format!("{GATE_VERSION}:{gate}"),
            arm: None,
            reason: reason.into(),
            matched_text: None,
            facts_used,
            other_arms: json!({}),
        }
    }

    pub fn to_json(&self) -> Value {
        json!({
            "open": self.open,
            "arm": self.arm,
            "gate_id": self.gate_id,
            "reason": self.reason,
            "matched_text": self.matched_text,
            "facts_used": self.facts_used,
            "other_arms": self.other_arms,
        })
    }
}

pub fn tail_chars(text: &str, n: usize) -> &str {
    let count = text.chars().count();
    if count <= n {
        return text;
    }
    let skip = count - n;
    let start = text.char_indices().nth(skip).map_or(0, |(i, _)| i);
    &text[start..]
}

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

pub fn contains_bounded(hay: &str, needle: &str) -> bool {
    let (Some(first), Some(last)) = (needle.chars().next(), needle.chars().last()) else {
        return false;
    };
    let (sw, ew) = (is_word(first), is_word(last));
    hay.match_indices(needle).any(|(start, _)| {
        let end = start + needle.len();
        let before = !sw || hay[..start].chars().next_back().is_none_or(|c| !is_word(c));
        let after = !ew || hay[end..].chars().next().is_none_or(|c| !is_word(c));
        before && after
    })
}

pub fn last_line(text: &str) -> &str {
    text.trim()
        .lines()
        .map(str::trim)
        .rfind(|l| !l.is_empty())
        .unwrap_or("")
}

fn find(re: &Regex, text: &str) -> Option<String> {
    re.find(text).map(|m| m.as_str().to_owned())
}

fn done_match(tail: &str, verifier_pattern: &str) -> Option<String> {
    let lowered: Vec<String> = verifier_pattern
        .lines()
        .map(|p| p.trim().to_lowercase())
        .filter(|p| !p.is_empty())
        .collect();
    lowered
        .iter()
        .find(|p| contains_bounded(tail, p))
        .cloned()
        .or_else(|| {
            DONE_VOCAB
                .iter()
                .find(|w| contains_bounded(tail, w))
                .map(|w| (*w).to_owned())
        })
}

pub fn phrase_gates(text: &str, verifier_pattern: &str) -> PhraseGates {
    let t = text.trim_end();
    let end600 = tail_chars(t, 600);
    let end250 = tail_chars(t, 250);
    let tail = tail_chars(t, 2500).to_lowercase();
    let fragment = t.ends_with(':') || t.ends_with('\u{2026}') || t.ends_with("...");
    let wait_match = find(&WAIT, tail_chars(end600, 400)).or_else(|| find(&NEXT_STEP, end250));
    let g2 = wait_match.is_some() || fragment;
    let question = last_line(t).ends_with('?');
    let handback_match = find(&HANDBACK, tail_chars(end600, 500));
    let g3 = handback_match.is_some() || question;
    let done = done_match(&tail, verifier_pattern);
    let g4b = ARMING_MARKERS.iter().any(|m| tail.contains(m));
    let ended_on = if handback_match.is_some() {
        EndedOn::Handback
    } else if question {
        EndedOn::Question
    } else if g2 {
        EndedOn::PromiseOrWait
    } else if DONE_VOCAB
        .iter()
        .any(|w| contains_bounded(&end600.to_lowercase(), w))
    {
        EndedOn::Report
    } else {
        EndedOn::Other
    };
    PhraseGates {
        g2,
        g3,
        g4: done.is_some(),
        g4b,
        ended_on,
        empty: t.is_empty(),
        handback_match,
        wait_match,
        done_match: done,
        step_class: None,
    }
}
