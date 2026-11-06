use axum::http::HeaderMap;
use serde_json::Value;

use crate::extract::content::extract_text_from_content;
use crate::store::models::is_well_formed_session_id;
use crate::types::RequestParams;

pub(crate) fn extract_model(body: &Value) -> Option<String> {
    body.get("model").and_then(|v| v.as_str()).map(String::from)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum SessionIdMiss {
    Missing,
    Malformed,
    NonUtf8,
}

pub(crate) fn derive_session_id(headers: &HeaderMap) -> Result<String, SessionIdMiss> {
    let header = headers
        .get("x-claude-code-session-id")
        .ok_or(SessionIdMiss::Missing)?;
    match header.to_str() {
        Ok(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() {
                Err(SessionIdMiss::Missing)
            } else if is_well_formed_session_id(trimmed) {
                Ok(trimmed.to_string())
            } else {
                Err(SessionIdMiss::Malformed)
            }
        }
        Err(_) => Err(SessionIdMiss::NonUtf8),
    }
}

pub(crate) fn extract_stream(body: &Value) -> bool {
    body.get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false)
}

pub(crate) fn extract_system_prompt(body: &Value) -> Option<String> {
    match body.get("system")? {
        Value::String(s) => Some(s.clone()),
        Value::Array(arr) => {
            let texts: Vec<&str> = arr
                .iter()
                .filter_map(|block| {
                    if block.get("type").and_then(|t| t.as_str()) == Some("text") {
                        block.get("text").and_then(|t| t.as_str())
                    } else {
                        None
                    }
                })
                .collect();
            if texts.is_empty() {
                None
            } else {
                Some(texts.join("\n"))
            }
        }
        _ => None,
    }
}

pub(crate) fn extract_last_user_message(body: &Value) -> String {
    body.get("messages")
        .and_then(|v| v.as_array())
        .and_then(|msgs| {
            msgs.iter()
                .rev()
                .find(|msg| msg.get("role").and_then(|r| r.as_str()) == Some("user"))
        })
        .map(extract_text_from_message)
        .unwrap_or_default()
}

fn extract_text_from_message(msg: &Value) -> String {
    msg.get("content")
        .map(extract_text_from_content)
        .unwrap_or_default()
}

pub(crate) fn extract_message_count(body: &Value) -> usize {
    body.get("messages")
        .and_then(|v| v.as_array())
        .map(|a| a.len())
        .unwrap_or(0)
}

pub(crate) fn extract_tool_names(body: &Value) -> Vec<String> {
    body.get("tools")
        .and_then(|v| v.as_array())
        .map(|tools| {
            tools
                .iter()
                .filter_map(|t| t.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default()
}

pub(crate) fn extract_params(body: &Value) -> RequestParams {
    let thinking = body.get("thinking");
    let thinking_mode = thinking
        .and_then(|t| t.get("type"))
        .and_then(|v| v.as_str())
        .map(String::from);
    let thinking_enabled = thinking_mode.as_deref().is_some_and(|m| m != "disabled");
    RequestParams {
        max_tokens: body
            .get("max_tokens")
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
        temperature: body
            .get("temperature")
            .and_then(|v| v.as_f64())
            .map(|v| v as f32),
        thinking_enabled,
        thinking_budget: thinking
            .and_then(|t| t.get("budget_tokens"))
            .and_then(|v| v.as_u64())
            .map(|v| v as u32),
        thinking_mode,
        effort: effective_effort(body),
    }
}

fn effort_of(holder: &Value) -> Option<&str> {
    holder
        .get("output_config")
        .and_then(|c| c.get("effort"))
        .and_then(|v| v.as_str())
}

fn effective_effort(body: &Value) -> Option<String> {
    let messages = body
        .get("messages")
        .and_then(|m| m.as_array())
        .map(Vec::as_slice)
        .unwrap_or_default();
    let newest_user = messages
        .iter()
        .rposition(|m| m.get("role").and_then(|r| r.as_str()) == Some("user"))
        .unwrap_or(messages.len());
    messages[..newest_user]
        .iter()
        .rev()
        .filter(|m| m.get("role").and_then(|r| r.as_str()) == Some("system"))
        .find_map(effort_of)
        .or_else(|| effort_of(body))
        .map(String::from)
}

#[cfg(test)]
#[path = "fields_counts_tests.rs"]
mod counts_tests;
#[cfg(test)]
#[path = "fields_tests.rs"]
mod tests;
