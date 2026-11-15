use serde_json::{Map, Value, json};

use super::questions::key;
use super::rules::{Rule, TurnTools, cut_at_whitespace, summary_lines};
use crate::llm::decisions::text::tail_chars;

const ASK_CHARS: usize = 600;
const END_CHARS: usize = 1_500;
const FILES_SHOWN: usize = 15;

pub struct TurnInput<'a> {
    pub operator_ask: &'a str,
    pub turn_end: &'a str,
    pub tools: &'a TurnTools,
}

pub fn state(input: &TurnInput<'_>, rules: &[&Rule]) -> Value {
    let by_key: Map<String, Value> = rules
        .iter()
        .enumerate()
        .map(|(i, rule)| (key(i), Value::String(rule.text.clone())))
        .collect();
    json!({
        "operator_ask": cut_at_whitespace(input.operator_ask, ASK_CHARS),
        "turn_end": tail_chars(input.turn_end, END_CHARS),
        "tool_calls": summary_lines(input.tools),
        "files_written": input.tools.paths.iter().take(FILES_SHOWN).collect::<Vec<_>>(),
        "rules": by_key,
    })
}
