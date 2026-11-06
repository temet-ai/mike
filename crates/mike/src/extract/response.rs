use serde_json::Value;

use super::content::extract_text_from_content;
use crate::types::TokenUsage;

pub(crate) fn extract_text_from_json(body: &Value) -> String {
    body.get("content")
        .map(extract_text_from_content)
        .unwrap_or_default()
}

pub(crate) fn extract_accumulated_text_from_body(body: &[u8]) -> String {
    serde_json::from_slice::<Value>(body)
        .ok()
        .map(|v| extract_text_from_json(&v))
        .unwrap_or_default()
}

pub(crate) fn extract_usage_from_message_start(data: &Value) -> Option<TokenUsage> {
    usage_from_object(data.pointer("/message/usage")?)
}

pub(crate) fn extract_completion_from_body(body: &[u8]) -> (Option<TokenUsage>, Option<String>) {
    let Ok(json) = serde_json::from_slice::<Value>(body) else {
        return (None, None);
    };
    let usage = json.get("usage").and_then(usage_from_object);
    let stop_reason = json
        .get("stop_reason")
        .and_then(Value::as_str)
        .map(str::to_owned);
    (usage, stop_reason)
}

fn usage_from_object(usage: &Value) -> Option<TokenUsage> {
    Some(TokenUsage {
        input_tokens: usage.get("input_tokens")?.as_u64()? as u32,
        output_tokens: usage
            .get("output_tokens")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as u32,
        cache_creation_input_tokens: usage
            .get("cache_creation_input_tokens")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
        cache_read_input_tokens: usage
            .get("cache_read_input_tokens")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
    })
}

pub(crate) fn extract_usage_from_message_delta(data: &Value) -> Option<TokenUsage> {
    let usage = data.get("usage")?;
    Some(TokenUsage {
        input_tokens: 0,
        output_tokens: usage.get("output_tokens")?.as_u64()? as u32,
        cache_creation_input_tokens: None,
        cache_read_input_tokens: None,
    })
}

const MESSAGE_START_STRUCTURAL_KEYS: [&str; 6] =
    ["id", "type", "role", "model", "content", "stop_reason"];

pub(crate) fn extract_message_start_fields(data: &Value) -> Option<Value> {
    let message = data.get("message")?.as_object()?;
    let fields: serde_json::Map<String, Value> = message
        .iter()
        .filter(|(k, _)| !MESSAGE_START_STRUCTURAL_KEYS.contains(&k.as_str()))
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    if fields.is_empty() {
        None
    } else {
        Some(Value::Object(fields))
    }
}

pub(crate) fn extract_message_delta_fields(data: &Value) -> Option<Value> {
    let mut fields = serde_json::Map::new();
    if let Some(stop_sequence) = data.pointer("/delta/stop_sequence") {
        fields.insert("stop_sequence".into(), stop_sequence.clone());
    }
    if let Some(usage) = data.get("usage").filter(|u| u.is_object()) {
        fields.insert("usage".into(), usage.clone());
    }
    if fields.is_empty() {
        None
    } else {
        Some(Value::Object(fields))
    }
}

pub(crate) fn merge_message_fields(existing: &mut Option<Value>, incoming: &Value) {
    let Some(incoming) = incoming.as_object() else {
        return;
    };
    let target = existing
        .get_or_insert_with(|| Value::Object(serde_json::Map::new()))
        .as_object_mut();
    let Some(target) = target else {
        return;
    };
    for (key, value) in incoming {
        match (target.get_mut(key), value) {
            (Some(Value::Object(old)), Value::Object(new)) if key == "usage" => {
                for (k, v) in new {
                    old.insert(k.clone(), v.clone());
                }
            }
            _ => {
                target.insert(key.clone(), value.clone());
            }
        }
    }
}

pub(crate) fn split_message_fields(fields: &Value) -> (Option<&Value>, Option<Value>) {
    let Some(object) = fields.as_object() else {
        return (None, None);
    };
    let rest: serde_json::Map<String, Value> = object
        .iter()
        .filter(|(k, _)| k.as_str() != "usage")
        .map(|(k, v)| (k.clone(), v.clone()))
        .collect();
    let rest = if rest.is_empty() {
        None
    } else {
        Some(Value::Object(rest))
    };
    (object.get("usage"), rest)
}

pub(crate) fn extract_stop_reason(data: &Value) -> Option<String> {
    data.pointer("/delta/stop_reason")
        .and_then(|v| v.as_str())
        .map(String::from)
}

pub(crate) fn extract_stop_details(data: &Value) -> Option<Value> {
    data.pointer("/delta/stop_details")
        .or_else(|| data.pointer("/stop_details"))
        .filter(|v| !v.is_null())
        .cloned()
}

pub(crate) fn extract_text_delta(data: &Value) -> Option<String> {
    data.pointer("/delta/text")
        .and_then(|v| v.as_str())
        .map(String::from)
}

pub(crate) fn extract_thinking_delta(data: &Value) -> Option<String> {
    data.pointer("/delta/thinking")
        .and_then(|v| v.as_str())
        .map(String::from)
}

pub(crate) fn extract_signature_delta(data: &Value) -> Option<String> {
    data.pointer("/delta/signature")
        .and_then(|v| v.as_str())
        .map(String::from)
}

pub(crate) fn extract_tool_use_start(data: &Value) -> Option<(String, String)> {
    let id = data.pointer("/content_block/id")?.as_str()?;
    let name = data.pointer("/content_block/name")?.as_str()?;
    Some((id.to_string(), name.to_string()))
}

pub(crate) fn extract_input_json_delta(data: &Value) -> Option<String> {
    data.pointer("/delta/partial_json")
        .and_then(|v| v.as_str())
        .map(String::from)
}

#[cfg(test)]
#[path = "response_tests.rs"]
mod tests;
