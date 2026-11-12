pub(crate) mod cache_gate;
pub(crate) mod content_blocks;
pub(crate) mod file_reads;
pub(crate) mod measure;
pub(crate) mod recovery;
pub(crate) mod shell_output;
pub(crate) mod shell_stage;
pub(crate) mod stage;

#[cfg(test)]
use bytes::Bytes;
use serde::{Deserialize, Serialize};
use serde_json::Value;

const DEFAULT_MIN_BLOCK_BYTES: usize = 256;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct CompressConfig {
    pub(crate) enabled: bool,

    pub(crate) min_block_bytes: usize,
}

impl Default for CompressConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            min_block_bytes: DEFAULT_MIN_BLOCK_BYTES,
        }
    }
}

impl CompressConfig {
    pub(crate) fn for_measurement(&self) -> Self {
        Self {
            enabled: true,
            min_block_bytes: self.min_block_bytes,
        }
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub(crate) struct BlockStat {
    pub(crate) index: usize,
    pub(crate) kind: &'static str,
    pub(crate) original_bytes: usize,
    pub(crate) compressed_bytes: usize,
}

#[derive(Debug)]
pub(crate) enum CompressOutcome {
    Unchanged,
    Compressed {
        #[cfg(test)]
        new_body: Bytes,
        original_len: usize,
        compressed_len: usize,
        #[cfg(test)]
        blocks: Vec<BlockStat>,
    },
}

pub(crate) trait Compressor {
    fn try_compress(&self, block: &mut Value) -> bool;
}

pub(crate) struct JsonMinify;

pub(crate) struct WhitespaceSquash;

impl Compressor for JsonMinify {
    fn try_compress(&self, block: &mut Value) -> bool {
        let Some(obj) = block.as_object_mut() else {
            return false;
        };
        let kind = obj.get("type").and_then(Value::as_str).unwrap_or("");
        match kind {
            "text" => minify_string_field(obj, "text"),
            "tool_result" => minify_tool_result_content(obj),
            _ => false,
        }
    }
}

fn minify_string_field(obj: &mut serde_json::Map<String, Value>, field: &str) -> bool {
    let Some(s) = obj.get(field).and_then(Value::as_str) else {
        return false;
    };
    let Some(compact) = compact_json_string(s) else {
        return false;
    };
    if compact.len() >= s.len() {
        return false;
    }
    obj.insert(field.to_string(), Value::String(compact));
    true
}

fn minify_tool_result_content(obj: &mut serde_json::Map<String, Value>) -> bool {
    match obj.get_mut("content") {
        Some(Value::String(s)) => {
            let Some(compact) = compact_json_string(s) else {
                return false;
            };
            if compact.len() >= s.len() {
                return false;
            }
            *s = compact;
            true
        }
        Some(Value::Array(parts)) => {
            let mut changed = false;
            for part in parts.iter_mut() {
                if let Some(part_obj) = part.as_object_mut()
                    && part_obj.get("type").and_then(Value::as_str) == Some("text")
                    && minify_string_field(part_obj, "text")
                {
                    changed = true;
                }
            }
            changed
        }
        _ => false,
    }
}

fn compact_json_string(s: &str) -> Option<String> {
    let parsed: Value = serde_json::from_str(s).ok()?;
    serde_json::to_string(&parsed).ok()
}

impl Compressor for WhitespaceSquash {
    fn try_compress(&self, block: &mut Value) -> bool {
        let Some(obj) = block.as_object_mut() else {
            return false;
        };
        if obj.get("type").and_then(Value::as_str) != Some("text") {
            return false;
        }
        let Some(text) = obj.get("text").and_then(Value::as_str) else {
            return false;
        };
        let squashed = squash_whitespace(text);
        if squashed.len() >= text.len() {
            return false;
        }
        obj.insert("text".to_string(), Value::String(squashed));
        true
    }
}

fn squash_whitespace(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for (i, line) in text.split('\n').enumerate() {
        if i > 0 {
            out.push('\n');
        }
        let line = line.strip_suffix('\r').unwrap_or(line);
        let mut prev_was_space = false;
        let mut line_buf = String::with_capacity(line.len());
        for ch in line.chars() {
            if ch == ' ' || ch == '\t' {
                if !prev_was_space {
                    line_buf.push(' ');
                }
                prev_was_space = true;
            } else {
                line_buf.push(ch);
                prev_was_space = false;
            }
        }
        out.push_str(line_buf.trim_end_matches(' '));
    }
    out
}

pub(crate) fn compress_body(raw: &[u8], cfg: &CompressConfig) -> CompressOutcome {
    if !cfg.enabled {
        return CompressOutcome::Unchanged;
    }
    content_blocks::measure_body(raw, cfg)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn json_minify_compacts_tool_result_json_string() {
        let mut block = json!({
            "type": "tool_result",
            "tool_use_id": "abc",
            "content": "{\n  \"a\": 1,\n  \"b\": [1, 2, 3]\n}"
        });
        assert!(JsonMinify.try_compress(&mut block));
        assert_eq!(block["content"], "{\"a\":1,\"b\":[1,2,3]}");
    }

    #[test]
    fn json_minify_compacts_text_block_when_text_is_json() {
        let mut block = json!({
            "type": "text",
            "text": "{\n   \"k\":    \"v\"\n}"
        });
        assert!(JsonMinify.try_compress(&mut block));
        assert_eq!(block["text"], "{\"k\":\"v\"}");
    }

    #[test]
    fn json_minify_compacts_tool_result_array_parts() {
        let mut block = json!({
            "type": "tool_result",
            "content": [
                {"type": "text", "text": "[1,    2,    3]"},
                {"type": "text", "text": "plain prose, not json"}
            ]
        });
        assert!(JsonMinify.try_compress(&mut block));
        assert_eq!(block["content"][0]["text"], "[1,2,3]");
        assert_eq!(block["content"][1]["text"], "plain prose, not json");
    }

    #[test]
    fn json_minify_skips_non_json_string() {
        let mut block = json!({
            "type": "tool_result",
            "content": "this is not json at all"
        });
        let before = block.clone();
        assert!(!JsonMinify.try_compress(&mut block));
        assert_eq!(block, before);
    }

    #[test]
    fn json_minify_skips_already_compact_json() {
        let mut block = json!({
            "type": "tool_result",
            "content": "{\"a\":1}"
        });
        let before = block.clone();
        assert!(!JsonMinify.try_compress(&mut block));
        assert_eq!(block, before);
    }

    #[test]
    fn json_minify_skips_non_object_value() {
        let mut block = json!([1, 2, 3]);
        assert!(!JsonMinify.try_compress(&mut block));
    }

    #[test]
    fn whitespace_squash_collapses_runs_and_trailing() {
        let mut block = json!({
            "type": "text",
            "text": "hello      world   \nfoo\t\tbar   "
        });
        assert!(WhitespaceSquash.try_compress(&mut block));
        assert_eq!(block["text"], "hello world\nfoo bar");
    }

    #[test]
    fn whitespace_squash_preserves_single_spaces_and_newlines() {
        let mut block = json!({
            "type": "text",
            "text": "a b\nc d"
        });
        let before = block.clone();
        assert!(!WhitespaceSquash.try_compress(&mut block));
        assert_eq!(block, before);
    }

    #[test]
    fn whitespace_squash_skips_non_text_block() {
        let mut block = json!({
            "type": "tool_result",
            "content": "x       y"
        });
        assert!(!WhitespaceSquash.try_compress(&mut block));
    }

    #[test]
    fn squash_whitespace_helper_handles_crlf() {
        assert_eq!(squash_whitespace("a   \r\nb"), "a\nb");
    }

    #[test]
    fn compact_json_string_returns_none_for_invalid() {
        assert!(compact_json_string("not { json").is_none());
        assert!(compact_json_string("{\"ok\":1}").is_some());
    }

    #[test]
    fn compress_body_disabled_is_unchanged() {
        let cfg = CompressConfig::default();
        assert!(!cfg.enabled);
        let body = br#"{"system":[{"type":"text","text":"hi"}]}"#;
        assert!(matches!(
            compress_body(body, &cfg),
            CompressOutcome::Unchanged
        ));
    }

    #[test]
    fn for_measurement_forces_enabled_keeps_min_block_bytes() {
        let cfg = CompressConfig {
            enabled: false,
            min_block_bytes: 999,
        };
        let eff = cfg.for_measurement();
        assert!(eff.enabled);
        assert_eq!(eff.min_block_bytes, 999);
    }
}
