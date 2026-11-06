use std::collections::HashMap;

use bytes::Bytes;
use serde_json::Value;
use serde_json::value::RawValue;

use super::block_walk::walk_blocks;
use super::matcher::block_matches;
use super::{FilterConfig, FilterOutcome, FilterRule, project_scope_matches};

pub(super) fn transform_body(raw: &[u8], cfg: &FilterConfig, project_path: &str) -> FilterOutcome {
    let active: Vec<&FilterRule> = cfg
        .rules
        .iter()
        .filter(|r| {
            r.enabled
                && r.project_path
                    .as_deref()
                    .is_none_or(|p| project_scope_matches(p, project_path))
        })
        .collect();
    if active.is_empty() {
        return FilterOutcome::Unchanged;
    }

    let mut value: Value = match serde_json::from_slice(raw) {
        Ok(v) => v,
        Err(_) => return FilterOutcome::Unchanged,
    };

    let mut firing_rule_ids: Vec<i64> = Vec::new();

    let Some(obj) = value.as_object_mut() else {
        return FilterOutcome::Unchanged;
    };
    let system_changed = if let Some(Value::Array(arr)) = obj.get_mut("system") {
        walk_blocks(arr, &active, &mut firing_rule_ids, true)
    } else {
        false
    };
    let mut messages_changed = false;
    if let Some(Value::Array(messages_arr)) = obj.get_mut("messages") {
        for msg in messages_arr.iter_mut() {
            if let Value::Object(msg_obj) = msg
                && let Some(Value::Array(arr)) = msg_obj.get_mut("content")
            {
                messages_changed |= walk_blocks(arr, &active, &mut firing_rule_ids, false);
            }
        }
    }

    if !system_changed && !messages_changed {
        return FilterOutcome::Unchanged;
    }

    let new_bytes = splice_changed_top_level_keys(raw, &value, system_changed, messages_changed)
        .or_else(|| serde_json::to_vec(&value).ok());
    let Some(new_bytes) = new_bytes else {
        return FilterOutcome::Unchanged;
    };

    let original_len = raw.len();
    let filtered_len = new_bytes.len();
    FilterOutcome::Rewrote {
        new_body: Bytes::from(new_bytes),
        original_len,
        filtered_len,
        firing_rule_ids,
    }
}

pub(crate) fn splice_changed_top_level_keys(
    raw: &[u8],
    value: &Value,
    system_changed: bool,
    messages_changed: bool,
) -> Option<Vec<u8>> {
    let mut keys: Vec<&str> = Vec::new();
    if system_changed {
        keys.push("system");
    }
    if messages_changed {
        keys.push("messages");
    }
    splice_top_level_keys(raw, value, &keys)
}

pub(crate) fn splice_top_level_keys(raw: &[u8], value: &Value, keys: &[&str]) -> Option<Vec<u8>> {
    let spans: HashMap<&str, &RawValue> = serde_json::from_slice(raw).ok()?;
    let base = raw.as_ptr() as usize;

    let mut replacements: Vec<(usize, usize, Vec<u8>)> = Vec::new();
    for key in keys {
        let raw_value = *spans.get(*key)?;
        let new_value = value.get(key)?;
        replacements.extend(key_replacements(
            raw.len(),
            base,
            raw_value.get(),
            new_value,
        )?);
    }

    replacements.sort_by_key(|(start, _, _)| *start);
    let mut out = Vec::with_capacity(raw.len());
    let mut cursor = 0usize;
    for (start, end, bytes) in replacements {
        if start < cursor {
            return None;
        }
        out.extend_from_slice(raw.get(cursor..start)?);
        out.extend_from_slice(&bytes);
        cursor = end;
    }
    out.extend_from_slice(raw.get(cursor..)?);
    Some(out)
}

type Replacement = (usize, usize, Vec<u8>);

fn span_of(raw_len: usize, base: usize, text: &str) -> Option<(usize, usize)> {
    let start = (text.as_ptr() as usize).checked_sub(base)?;
    let end = start.checked_add(text.len())?;
    (end <= raw_len).then_some((start, end))
}

fn key_replacements(
    raw_len: usize,
    base: usize,
    span_text: &str,
    new_value: &Value,
) -> Option<Vec<Replacement>> {
    let (start, end) = span_of(raw_len, base, span_text)?;
    let Some(new_items) = new_value.as_array() else {
        return Some(vec![(start, end, serde_json::to_vec(new_value).ok()?)]);
    };
    let old_items: Vec<&RawValue> = match serde_json::from_str(span_text) {
        Ok(items) => items,
        Err(_) => return Some(vec![(start, end, serde_json::to_vec(new_value).ok()?)]),
    };
    let Some(last_old) = old_items.last() else {
        return Some(vec![(start, end, serde_json::to_vec(new_value).ok()?)]);
    };
    if new_items.len() < old_items.len() {
        return Some(vec![(start, end, serde_json::to_vec(new_value).ok()?)]);
    }
    let mut out = Vec::new();
    for (old, new) in old_items.iter().zip(new_items) {
        let old_value: Value = serde_json::from_str(old.get()).ok()?;
        if old_value != *new {
            let (s, e) = span_of(raw_len, base, old.get())?;
            out.push((s, e, serde_json::to_vec(new).ok()?));
        }
    }
    let added = new_items.get(old_items.len()..).unwrap_or_default();
    if !added.is_empty() {
        let (_, after_last) = span_of(raw_len, base, last_old.get())?;
        let mut tail = Vec::new();
        for item in added {
            tail.push(b',');
            tail.extend(serde_json::to_vec(item).ok()?);
        }
        out.push((after_last, after_last, tail));
    }
    Some(out)
}

pub(super) fn is_text_block(block: &Value) -> bool {
    block
        .get("type")
        .and_then(|v| v.as_str())
        .map(|t| t == "text")
        .unwrap_or(false)
}

pub(super) fn anchor_matches_any_block(rule: &FilterRule, raw: &[u8]) -> bool {
    let Ok(value) = serde_json::from_slice::<Value>(raw) else {
        return false;
    };
    let Some(obj) = value.as_object() else {
        return false;
    };
    if let Some(Value::Array(arr)) = obj.get("system")
        && arr.iter().any(|b| block_text_matches(rule, b))
    {
        return true;
    }
    if let Some(Value::Array(messages)) = obj.get("messages") {
        for msg in messages {
            let Value::Object(msg_obj) = msg else {
                continue;
            };
            if let Some(Value::Array(arr)) = msg_obj.get("content")
                && arr.iter().any(|b| block_text_matches(rule, b))
            {
                return true;
            }
        }
    }
    false
}

fn block_text_matches(rule: &FilterRule, block: &Value) -> bool {
    if !is_text_block(block) {
        return false;
    }
    let text = block.get("text").and_then(|v| v.as_str()).unwrap_or("");
    block_matches(rule, text)
}
