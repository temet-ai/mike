use serde::Deserialize;
use serde_json::{Value, json};

use crate::store::persistence::supplied_items::PendingVerdict;

const PROMPT_CHARS: usize = 4000;
const REPLY_CHARS: usize = 6000;
const NEXT_CHARS: usize = 2000;

pub const VERDICTS: [&str; 3] = ["used", "corrected_anyway", "ignored"];

pub use crate::settings::meta_memory_loop::SUPPLY_VERDICT_PROMPT as SYSTEM;

#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct Answer {
    pub verdict: String,
    pub rationale: String,
    pub evidence: String,
}

pub fn schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "required": ["verdict", "rationale", "evidence"],
        "properties": {
            "verdict": {"type": "string", "enum": VERDICTS},
            "rationale": {"type": "string"},
            "evidence": {"type": "string"}
        }
    })
}

pub fn user_message(item: &PendingVerdict) -> String {
    let mut out = String::new();
    block(
        &mut out,
        &format!("memory ({}, {})", item.item_kind, item.route),
        &item.item_text,
    );
    block(
        &mut out,
        "operator prompt",
        &head(item.user_prompt.as_deref().unwrap_or(""), PROMPT_CHARS),
    );
    block(&mut out, "tools used", &item.tools_used.join(", "));
    block(&mut out, "files touched", &item.files_touched.join("\n"));
    block(
        &mut out,
        "final reply",
        &tail(
            item.final_assistant_text.as_deref().unwrap_or(""),
            REPLY_CHARS,
        ),
    );
    block(
        &mut out,
        "turn reaction recorded by the turn judge",
        item.reaction.as_deref().unwrap_or("none"),
    );
    block(
        &mut out,
        "operator's next message",
        &head(item.next_message.as_deref().unwrap_or("(none)"), NEXT_CHARS),
    );
    out
}

fn block(out: &mut String, name: &str, text: &str) {
    out.push_str(&format!("<<<{name}>>>\n{text}\n<<<end {name}>>>\n\n"));
}

fn head(text: &str, max: usize) -> String {
    let total = text.chars().count();
    if total <= max {
        return text.to_owned();
    }
    let kept: String = text.chars().take(max).collect();
    format!("{kept}\n[the last {} characters are left out]", total - max)
}

fn tail(text: &str, max: usize) -> String {
    let total = text.chars().count();
    if total <= max {
        return text.to_owned();
    }
    let kept: String = text.chars().skip(total - max).collect();
    format!(
        "[the first {} characters are left out]\n{kept}",
        total - max
    )
}

pub fn valid(answer: &Answer) -> bool {
    VERDICTS.contains(&answer.verdict.as_str())
}
