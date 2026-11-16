use serde_json::Value;

use super::prior::{role_is, text_of};

const SCHEDULERS: &[&str] = &["CronCreate", "ScheduleWakeup"];
const PREFIX_CHARS: usize = 200;
const ENTRYPOINT: &str = "cc_entrypoint=";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Signals {
    pub sent_before: usize,
    pub scheduled_by: Option<String>,
    pub entrypoint: Option<String>,
}

fn normal(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn same_prompt(a: &str, b: &str) -> bool {
    let (a, b) = (normal(a), normal(b));
    if a.is_empty() || b.is_empty() {
        return false;
    }
    let long = |s: &str| s.chars().count() >= PREFIX_CHARS;
    a == b
        || (long(&a)
            && long(&b)
            && a.chars()
                .take(PREFIX_CHARS)
                .eq(b.chars().take(PREFIX_CHARS)))
}

fn scheduled_prompts(messages: &[Value]) -> Vec<(&str, &str)> {
    messages
        .iter()
        .filter(|m| role_is(m, "assistant"))
        .filter_map(|m| m.get("content").and_then(Value::as_array))
        .flatten()
        .filter(|block| block.get("type").and_then(Value::as_str) == Some("tool_use"))
        .filter_map(|block| {
            let name = block.get("name").and_then(Value::as_str)?;
            let tool = SCHEDULERS.iter().find(|s| **s == name)?;
            let prompt = block.get("input")?.get("prompt")?.as_str()?;
            Some((*tool, prompt))
        })
        .collect()
}

fn system_texts(body: &Value) -> Vec<&str> {
    match body.get("system") {
        Some(Value::String(text)) => vec![text.as_str()],
        Some(Value::Array(blocks)) => blocks
            .iter()
            .filter_map(|b| b.get("text").and_then(Value::as_str))
            .collect(),
        _ => Vec::new(),
    }
}

fn entrypoint(body: &Value) -> Option<String> {
    system_texts(body).into_iter().find_map(|text| {
        let at = text.find(ENTRYPOINT)? + ENTRYPOINT.len();
        let value: String = text[at..]
            .chars()
            .take_while(|c| !matches!(c, ';' | ' ' | '\n'))
            .collect();
        (!value.is_empty()).then_some(value)
    })
}

pub fn current_text(body: &[u8]) -> String {
    serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|parsed| {
            parsed
                .get("messages")?
                .as_array()?
                .iter()
                .rev()
                .filter(|m| role_is(m, "user"))
                .map(text_of)
                .find(|t| !t.is_empty())
        })
        .unwrap_or_default()
}

pub fn signals(body: &[u8], text: &str) -> Signals {
    let Ok(parsed) = serde_json::from_slice::<Value>(body) else {
        return Signals::default();
    };
    let messages = parsed
        .get("messages")
        .and_then(Value::as_array)
        .map_or(&[][..], Vec::as_slice);
    let users: Vec<String> = messages
        .iter()
        .filter(|m| role_is(m, "user"))
        .map(text_of)
        .filter(|t| !t.is_empty())
        .collect();
    let earlier = users.len().saturating_sub(1);
    Signals {
        sent_before: users[..earlier]
            .iter()
            .filter(|t| same_prompt(t, text))
            .count(),
        scheduled_by: scheduled_prompts(messages)
            .into_iter()
            .find(|(_, prompt)| same_prompt(prompt, text))
            .map(|(tool, _)| tool.to_owned()),
        entrypoint: entrypoint(&parsed),
    }
}
