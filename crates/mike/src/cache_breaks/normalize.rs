use serde_json::{Map, Value};

const BILLING_HEADER: &str = "x-anthropic-billing-header";

fn is_billing_header(block: &Value) -> bool {
    block
        .get("text")
        .and_then(Value::as_str)
        .is_some_and(|t| t.starts_with(BILLING_HEADER))
}

fn strip(value: &Value) -> Value {
    match value {
        Value::Object(map) => {
            let mut out: Map<String, Value> = map
                .iter()
                .filter(|(k, _)| k.as_str() != "cache_control")
                .map(|(k, v)| (k.clone(), strip(v)))
                .collect();
            if let Some(Value::String(text)) = out.get("content") {
                let block = serde_json::json!([{"type": "text", "text": text}]);
                out.insert("content".to_owned(), block);
            }
            Value::Object(out)
        }
        Value::Array(items) => Value::Array(items.iter().map(strip).collect()),
        other => other.clone(),
    }
}

pub(crate) fn system(body: &Value) -> Value {
    match body.get("system") {
        Some(Value::Array(blocks)) => Value::Array(
            blocks
                .iter()
                .filter(|b| !is_billing_header(b))
                .map(strip)
                .collect(),
        ),
        Some(other) => strip(other),
        None => Value::Null,
    }
}

pub(crate) fn tools(body: &Value) -> Value {
    body.get("tools").map_or(Value::Null, strip)
}

pub(crate) fn messages(body: &Value) -> Vec<Value> {
    body.get("messages")
        .and_then(Value::as_array)
        .map(|ms| ms.iter().map(strip).collect())
        .unwrap_or_default()
}

pub(crate) fn tool_names(body: &Value) -> Vec<String> {
    body.get("tools")
        .and_then(Value::as_array)
        .map(|ts| {
            ts.iter()
                .filter_map(|t| t.get("name").and_then(Value::as_str))
                .map(str::to_owned)
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn is_continuation(body: &Value) -> bool {
    body.pointer("/thread/type").and_then(Value::as_str) == Some("continue")
}

pub(crate) fn previous_message_id(body: &Value) -> Option<&str> {
    body.pointer("/thread/previous_message_id")
        .and_then(Value::as_str)
}

pub(crate) fn find_slice(haystack: &[Value], needle: &[Value]) -> Option<usize> {
    if needle.len() > haystack.len() {
        return None;
    }
    (0..=haystack.len() - needle.len())
        .rev()
        .find(|&k| haystack[k..k + needle.len()] == *needle)
}
