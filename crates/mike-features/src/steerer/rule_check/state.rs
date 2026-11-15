use serde_json::{Map, Value, json};

use super::questions::key;
use super::rules::{Rule, TurnTools, listed_lines};

const FILES_SHOWN: usize = 15;

pub struct TurnInput<'a> {
    pub tools: &'a TurnTools,
}

pub fn state(input: &TurnInput<'_>, rules: &[&Rule]) -> Value {
    let by_key: Map<String, Value> = rules
        .iter()
        .enumerate()
        .map(|(i, rule)| (key(i), Value::String(rule.text.clone())))
        .collect();
    json!({
        "tool_calls": listed_lines(input.tools, rules),
        "files_written": input.tools.paths.iter().take(FILES_SHOWN).collect::<Vec<_>>(),
        "rules": by_key,
    })
}
