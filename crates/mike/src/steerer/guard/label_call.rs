use std::collections::BTreeMap;
use std::fmt::Write as _;

use serde::Deserialize;
use serde_json::{Value, json};

use super::Question;
use super::gates::tail_chars;
use super::input::ENDING_CHARS;
use crate::llm::OpenRouterClient;

pub const LABELLER_PROMPT: &str = include_str!("prompt/labeller.md");

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LabelAnswer {
    pub corrects_this: String,
    #[serde(default)]
    pub quote: String,
}

#[derive(Debug, Clone, PartialEq, Deserialize, Default)]
pub struct LabelVerdict {
    #[serde(default)]
    pub questions: BTreeMap<String, LabelAnswer>,
    #[serde(default)]
    pub reaction: Option<String>,
}

pub struct LabelRequest<'a> {
    pub ending: &'a str,
    pub next_message: &'a str,
    pub correction: Option<&'a str>,
    pub questions: &'a [Question],
    pub want_reaction: bool,
}

pub fn label_schema(questions: &[Question], want_reaction: bool) -> Value {
    let answer = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "corrects_this": {"type": "string", "enum": ["yes", "no", "unclear"]},
            "quote": {"type": "string"},
        },
        "required": ["corrects_this", "quote"],
    });
    let props: serde_json::Map<String, Value> = questions
        .iter()
        .map(|q| (q.code().to_owned(), answer.clone()))
        .collect();
    let required: Vec<&str> = questions.iter().map(|q| q.code()).collect();
    let mut root = json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "questions": {"type": "object", "additionalProperties": false, "properties": props, "required": required},
        },
        "required": ["questions"],
    });
    if want_reaction {
        root["properties"]["reaction"] = json!({"type": "string", "enum": ["corrected", "redirected", "accepted", "no_response", "unclear"]});
        root["required"] = json!(["questions", "reaction"]);
    }
    root
}

pub fn label_input(req: &LabelRequest<'_>) -> String {
    let mut out = String::new();
    let codes: Vec<&str> = req.questions.iter().map(|q| q.code()).collect();
    let _ = writeln!(out, "Questions: {}\n", codes.join(", "));
    let _ = write!(
        out,
        "## ENDING\n{}\n\n",
        tail_chars(req.ending, ENDING_CHARS)
    );
    let _ = write!(out, "## NEXT MESSAGE\n{}\n\n", req.next_message);
    if let Some(c) = req.correction {
        let _ = write!(out, "## EARLIER CORRECTION\n{c}\n\n");
    }
    if req.want_reaction {
        out.push_str("Also return reaction.\n");
    }
    out
}

pub async fn call_labeller(
    llm: &OpenRouterClient,
    req: &LabelRequest<'_>,
) -> Result<LabelVerdict, String> {
    llm.verdict_with_usage::<LabelVerdict>(
        LABELLER_PROMPT,
        &label_input(req),
        label_schema(req.questions, req.want_reaction),
    )
    .await
    .map(|(v, _)| v)
    .map_err(|e| e.to_string())
}
