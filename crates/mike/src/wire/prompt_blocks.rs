use serde_json::Value;
use sha2::{Digest, Sha256};

pub(crate) fn signature_hash_for_unknown_blocks(value: &Value) -> String {
    let canonical = canonicalize_json(value);
    let mut hasher = Sha256::new();
    hasher.update(canonical.as_bytes());
    hex::encode(hasher.finalize())
}

fn canonicalize_json(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => n.to_string(),
        Value::String(s) => match serde_json::to_string(s) {
            Ok(encoded) => encoded,
            Err(_) => format!("\"{s}\""),
        },
        Value::Array(items) => {
            let mut out = String::from("[");
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&canonicalize_json(item));
            }
            out.push(']');
            out
        }
        Value::Object(map) => {
            let mut entries: Vec<(&String, &Value)> = map.iter().collect();
            entries.sort_by(|a, b| a.0.cmp(b.0));
            let mut out = String::from("{");
            for (i, (key, val)) in entries.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                let encoded_key =
                    serde_json::to_string(key.as_str()).unwrap_or_else(|_| format!("\"{key}\""));
                out.push_str(&encoded_key);
                out.push(':');
                out.push_str(&canonicalize_json(val));
            }
            out.push('}');
            out
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum UnknownShapeReason {
    Missing,
    StringForm,
    NonThreeBlock { len: usize },
    NonTextBlock { index: usize },
    Block2TextMissing,
    NoIdentityBlock { len: usize },
}

impl UnknownShapeReason {
    pub(crate) fn to_json(&self) -> Value {
        match self {
            Self::Missing => serde_json::json!({"reason": "missing"}),
            Self::StringForm => serde_json::json!({"reason": "string_form"}),
            Self::NonThreeBlock { len } => {
                serde_json::json!({"reason": "non_3_block", "len": *len})
            }
            Self::NonTextBlock { index } => {
                serde_json::json!({"reason": "non_text_block", "index": *index})
            }
            Self::Block2TextMissing => serde_json::json!({"reason": "block_2_text_missing"}),
            Self::NoIdentityBlock { len } => {
                serde_json::json!({"reason": "no_identity_block", "len": *len})
            }
        }
    }
}

#[derive(Debug, Clone)]
pub(crate) struct BlockTriple {
    pub(crate) block2_text: String,
}

pub(crate) fn parse_system_blocks(body: &Value) -> Result<BlockTriple, UnknownShapeReason> {
    let system = match body.get("system") {
        Some(s) if !s.is_null() => s,
        _ => return Err(UnknownShapeReason::Missing),
    };

    if system.is_string() {
        return Err(UnknownShapeReason::StringForm);
    }

    let arr = match system.as_array() {
        Some(arr) => arr,
        None => {
            return Err(UnknownShapeReason::Missing);
        }
    };

    if arr.len() != 3 {
        return Err(UnknownShapeReason::NonThreeBlock { len: arr.len() });
    }

    for (i, block) in arr.iter().enumerate() {
        let is_text_block = block
            .get("type")
            .and_then(|v| v.as_str())
            .map(|t| t == "text")
            .unwrap_or(false);
        if !is_text_block {
            return Err(UnknownShapeReason::NonTextBlock { index: i });
        }
    }

    let block2_text = match arr[2].get("text").and_then(|v| v.as_str()) {
        Some(s) => s.to_string(),
        None => return Err(UnknownShapeReason::Block2TextMissing),
    };

    Ok(BlockTriple { block2_text })
}

pub(crate) const ROLE_BODY_STRIP_INDEX: u8 = 2;

const BILLING_HEADER_PREFIX: &str = "x-anthropic-billing-header:";

#[derive(Debug, Clone)]
pub(crate) struct IdentityBlocks {
    pub(crate) role_body_text: String,
}

pub(crate) fn parse_identity_blocks(body: &Value) -> Result<IdentityBlocks, UnknownShapeReason> {
    let system = match body.get("system") {
        Some(s) if !s.is_null() => s,
        _ => return Err(UnknownShapeReason::Missing),
    };

    if let Some(text) = system.as_str() {
        return Ok(IdentityBlocks {
            role_body_text: text.to_string(),
        });
    }

    let arr = match system.as_array() {
        Some(arr) => arr,
        None => return Err(UnknownShapeReason::Missing),
    };

    for (i, block) in arr.iter().enumerate() {
        let is_text_block = block
            .get("type")
            .and_then(|v| v.as_str())
            .is_some_and(|t| t == "text");
        if !is_text_block {
            return Err(UnknownShapeReason::NonTextBlock { index: i });
        }
    }

    let has_billing_header = arr
        .first()
        .and_then(|b| b.get("text"))
        .and_then(Value::as_str)
        .is_some_and(|t| t.trim_start().starts_with(BILLING_HEADER_PREFIX));
    let first_identity = usize::from(has_billing_header);
    let role_body_index = match arr.len().checked_sub(1) {
        Some(last) if last >= first_identity => last,
        _ => return Err(UnknownShapeReason::NoIdentityBlock { len: arr.len() }),
    };

    let role_body_text = match arr[role_body_index].get("text").and_then(Value::as_str) {
        Some(s) => s.to_string(),
        None => return Err(UnknownShapeReason::Block2TextMissing),
    };

    Ok(IdentityBlocks { role_body_text })
}

pub(crate) fn block2_is_globally_scoped(body: &Value) -> bool {
    body.get("system")
        .and_then(Value::as_array)
        .and_then(|arr| arr.get(2))
        .is_some_and(crate::cache_scope::is_globally_scoped)
}

#[cfg(test)]
#[path = "prompt_blocks_tests.rs"]
mod prompt_blocks_tests;
