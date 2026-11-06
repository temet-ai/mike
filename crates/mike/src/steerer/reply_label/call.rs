use std::fmt::Write as _;

use serde::Deserialize;
use serde_json::{Value, json};

use crate::llm::OpenRouterClient;
use crate::store::persistence::tool_call_reads::PairedToolCallRow;

pub(crate) const REPLY_LABEL_PROMPT: &str = include_str!("prompt.md");
const TURN_CHARS: usize = 2_500;
const REPLY_START_CHARS: usize = 1_500;
const REPLY_END_CHARS: usize = 1_500;
const TOOL_INPUT_CHARS: usize = 400;
const TOOL_RESULT_CHARS: usize = 300;
const TOOLS_SHOWN: usize = 8;
const RULE_CHARS: usize = 4_000;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub(crate) struct ReplyVerdict {
    pub(crate) note_correct: String,
    pub(crate) reaction: String,
    #[serde(default)]
    pub(crate) failure: String,
    #[serde(default)]
    pub(crate) quote: String,
}

pub(crate) struct ReplyRequest<'a> {
    pub(crate) steerer: &'a str,
    pub(crate) rule: &'a str,
    pub(crate) note: &'a str,
    pub(crate) turn_text: &'a str,
    pub(crate) turn_tools: &'a [PairedToolCallRow],
    pub(crate) reply_text: &'a str,
    pub(crate) reply_tools: &'a [PairedToolCallRow],
}

pub(crate) fn reply_schema() -> Value {
    json!({
        "type": "object",
        "additionalProperties": false,
        "properties": {
            "note_correct": {"type": "string", "enum": ["yes", "no", "partly", "unclear"]},
            "reaction": {"type": "string", "enum": ["complied", "declined", "ignored"]},
            "failure": {"type": "string"},
            "quote": {"type": "string"},
        },
        "required": ["note_correct", "reaction", "failure", "quote"],
    })
}

pub(crate) fn label_for(note_correct: &str) -> &'static str {
    match note_correct {
        "yes" => "right",
        "no" => "wrong",
        _ => "indeterminate",
    }
}

fn head(text: &str, max: usize) -> &str {
    text.char_indices()
        .nth(max)
        .map_or(text, |(i, _)| &text[..i])
}

fn tail(text: &str, max: usize) -> &str {
    let count = text.chars().count();
    if count <= max {
        return text;
    }
    text.char_indices()
        .nth(count - max)
        .map_or(text, |(i, _)| &text[i..])
}

fn compact(value: Option<&Value>) -> String {
    match value {
        Some(Value::String(s)) => s.clone(),
        Some(v) => v.to_string(),
        None => String::new(),
    }
}

fn write_tools(out: &mut String, tools: &[PairedToolCallRow]) {
    if tools.is_empty() {
        out.push_str("(no tool calls)\n");
        return;
    }
    for t in tools.iter().take(TOOLS_SHOWN) {
        let input = compact(t.input.as_ref());
        let result = compact(t.result_content.as_ref());
        let error = if t.result_is_error == Some(true) {
            " [error]"
        } else {
            ""
        };
        let _ = writeln!(
            out,
            "- {}{error}: {}\n  result: {}",
            t.tool_name,
            head(&input, TOOL_INPUT_CHARS),
            head(&result, TOOL_RESULT_CHARS)
        );
    }
}

fn write_reply(out: &mut String, reply: &str) {
    if reply.chars().count() <= REPLY_START_CHARS + REPLY_END_CHARS {
        let _ = write!(out, "\n## SESSION'S REPLY (whole)\n{reply}\n\n");
        return;
    }
    let _ = write!(
        out,
        "\n## SESSION'S REPLY (start)\n{}\n\n## SESSION'S REPLY (end)\n{}\n\n",
        head(reply, REPLY_START_CHARS),
        tail(reply, REPLY_END_CHARS)
    );
}

pub(crate) fn reply_input(req: &ReplyRequest<'_>) -> String {
    let mut out = String::new();
    let _ = write!(out, "Steerer: {}\n\n", req.steerer);
    if !req.rule.trim().is_empty() {
        let _ = write!(
            out,
            "## THE STEERER'S RULE\n{}\n\n",
            head(req.rule, RULE_CHARS)
        );
    }
    let _ = write!(out, "## NOTE\n{}\n\n", req.note);
    let _ = write!(
        out,
        "## TURN THAT TRIGGERED IT (end)\n{}\n\n",
        tail(req.turn_text, TURN_CHARS)
    );
    out.push_str("### Turn tool calls\n");
    write_tools(&mut out, req.turn_tools);
    write_reply(&mut out, req.reply_text);
    out.push_str("### First tool calls of the reply\n");
    write_tools(&mut out, req.reply_tools);
    out
}

pub(crate) async fn call_reply_labeller(
    llm: &OpenRouterClient,
    req: &ReplyRequest<'_>,
) -> Result<ReplyVerdict, String> {
    llm.verdict_with_usage::<ReplyVerdict>(REPLY_LABEL_PROMPT, &reply_input(req), reply_schema())
        .await
        .map(|(v, _)| v)
        .map_err(|e| e.to_string())
}
