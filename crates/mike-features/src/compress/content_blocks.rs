#[cfg(any(test, feature = "test-seams"))]
use bytes::Bytes;
use serde_json::Value;

use super::{BlockStat, CompressConfig, CompressOutcome, Compressor, JsonMinify, WhitespaceSquash};

pub fn measure_body(raw: &[u8], cfg: &CompressConfig) -> CompressOutcome {
    let mut value: Value = match serde_json::from_slice(raw) {
        Ok(v) => v,
        Err(_) => return CompressOutcome::Unchanged,
    };

    let Some(obj) = value.as_object_mut() else {
        return CompressOutcome::Unchanged;
    };

    let compressors: [&dyn Compressor; 2] = [&JsonMinify, &WhitespaceSquash];

    let mut blocks: Vec<BlockStat> = Vec::new();
    let mut flat_index: usize = 0;

    if let Some(Value::Array(arr)) = obj.get_mut("system") {
        walk_blocks(arr, cfg, &compressors, &mut blocks, &mut flat_index);
    }
    if let Some(Value::Array(messages_arr)) = obj.get_mut("messages") {
        for msg in messages_arr.iter_mut() {
            if let Value::Object(msg_obj) = msg
                && let Some(Value::Array(arr)) = msg_obj.get_mut("content")
            {
                walk_blocks(arr, cfg, &compressors, &mut blocks, &mut flat_index);
            }
        }
    }

    if blocks.is_empty() {
        return CompressOutcome::Unchanged;
    }

    let new_bytes = match serde_json::to_vec(&value) {
        Ok(b) => b,
        Err(_) => return CompressOutcome::Unchanged,
    };

    CompressOutcome::Compressed {
        original_len: raw.len(),
        compressed_len: new_bytes.len(),
        #[cfg(any(test, feature = "test-seams"))]
        new_body: Bytes::from(new_bytes),
        #[cfg(any(test, feature = "test-seams"))]
        blocks,
    }
}

fn walk_blocks(
    arr: &mut [Value],
    cfg: &CompressConfig,
    compressors: &[&dyn Compressor; 2],
    blocks: &mut Vec<BlockStat>,
    flat_index: &mut usize,
) {
    for block in arr.iter_mut() {
        let index = *flat_index;
        *flat_index += 1;

        let kind = block_kind(block);
        let kind = match kind {
            Some(k) => k,
            None => continue,
        };

        let original_bytes = compressible_payload_len(block);
        if original_bytes < cfg.min_block_bytes {
            continue;
        }

        let mut changed = false;
        for compressor in compressors {
            if compressor.try_compress(block) {
                changed = true;
                break;
            }
        }
        if !changed {
            continue;
        }

        let compressed_bytes = compressible_payload_len(block);
        blocks.push(BlockStat {
            index,
            kind,
            original_bytes,
            compressed_bytes,
        });
    }
}

fn block_kind(block: &Value) -> Option<&'static str> {
    match block.get("type").and_then(Value::as_str) {
        Some("text") => Some("text"),
        Some("tool_result") => Some("tool_result"),
        _ => None,
    }
}

fn compressible_payload_len(block: &Value) -> usize {
    let Some(obj) = block.as_object() else {
        return 0;
    };
    match obj.get("type").and_then(Value::as_str) {
        Some("text") => obj.get("text").and_then(Value::as_str).map_or(0, str::len),
        Some("tool_result") => match obj.get("content") {
            Some(Value::String(s)) => s.len(),
            Some(Value::Array(parts)) => parts
                .iter()
                .filter_map(|p| p.get("text").and_then(Value::as_str))
                .map(str::len)
                .sum(),
            _ => 0,
        },
        _ => 0,
    }
}

#[cfg(test)]
pub mod tests {
    use super::super::{CompressConfig, CompressOutcome};
    use super::measure_body;
    use serde_json::{Value, json};

    fn cfg(min_block_bytes: usize) -> CompressConfig {
        CompressConfig {
            enabled: true,
            min_block_bytes,
        }
    }

    fn body_bytes(v: Value) -> anyhow::Result<Vec<u8>> {
        Ok(serde_json::to_vec(&v)?)
    }

    #[test]
    fn compresses_large_tool_result_json_in_messages() -> anyhow::Result<()> {
        let big_json = serde_json::to_string_pretty(&json!({
            "rows": (0..50).map(|i| json!({"id": i, "name": format!("row-{i}")})).collect::<Vec<_>>()
        }))?;
        let body = body_bytes(json!({
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "tool_result", "tool_use_id": "t1", "content": big_json}
                ]
            }]
        }))?;
        match measure_body(&body, &cfg(256)) {
            CompressOutcome::Compressed {
                blocks,
                original_len,
                compressed_len,
                ..
            } => {
                assert_eq!(blocks.len(), 1);
                assert_eq!(blocks[0].kind, "tool_result");
                assert!(
                    blocks[0].compressed_bytes < blocks[0].original_bytes,
                    "tool_result block must shrink"
                );
                assert!(compressed_len < original_len, "whole body must shrink");
            }
            CompressOutcome::Unchanged => {
                return Err(anyhow::anyhow!("expected Compressed"));
            }
        }
        Ok(())
    }

    #[test]
    fn small_blocks_below_threshold_are_skipped() -> anyhow::Result<()> {
        let body = body_bytes(json!({
            "system": [{"type": "text", "text": "short   text"}]
        }))?;
        assert!(matches!(
            measure_body(&body, &cfg(4096)),
            CompressOutcome::Unchanged
        ));
        Ok(())
    }

    #[test]
    fn whitespace_squash_fires_on_large_prose_block() -> anyhow::Result<()> {
        let prose = "lorem    ipsum   ".repeat(40);
        let body = body_bytes(json!({
            "system": [{"type": "text", "text": prose, "cache_control": {"type": "ephemeral"}}]
        }))?;
        match measure_body(&body, &cfg(256)) {
            CompressOutcome::Compressed {
                blocks, new_body, ..
            } => {
                assert_eq!(blocks.len(), 1);
                assert_eq!(blocks[0].kind, "text");
                let v: Value = serde_json::from_slice(&new_body)?;
                assert_eq!(v["system"][0]["cache_control"]["type"], "ephemeral");
            }
            CompressOutcome::Unchanged => {
                return Err(anyhow::anyhow!("expected Compressed"));
            }
        }
        Ok(())
    }

    #[test]
    fn non_text_blocks_passthrough_and_keep_index_stable() -> anyhow::Result<()> {
        let big_json = serde_json::to_string_pretty(&json!({"k": "v".repeat(500)}))?;
        let body = body_bytes(json!({
            "messages": [{
                "role": "user",
                "content": [
                    {"type": "image", "source": {"type": "base64", "data": "AAAA"}},
                    {"type": "tool_result", "content": big_json}
                ]
            }]
        }))?;
        match measure_body(&body, &cfg(256)) {
            CompressOutcome::Compressed { blocks, .. } => {
                assert_eq!(blocks.len(), 1);
                assert_eq!(blocks[0].index, 1, "image block consumed index 0");
            }
            CompressOutcome::Unchanged => {
                return Err(anyhow::anyhow!("expected Compressed"));
            }
        }
        Ok(())
    }

    #[test]
    fn malformed_json_returns_unchanged() {
        assert!(matches!(
            measure_body(b"not json {{{", &cfg(0)),
            CompressOutcome::Unchanged
        ));
    }

    #[test]
    fn non_object_body_returns_unchanged() -> anyhow::Result<()> {
        let body = body_bytes(json!([1, 2, 3]))?;
        assert!(matches!(
            measure_body(&body, &cfg(0)),
            CompressOutcome::Unchanged
        ));
        Ok(())
    }

    #[test]
    fn no_compressible_blocks_returns_unchanged() -> anyhow::Result<()> {
        let compact = serde_json::to_string(&json!({"a": (0..200).collect::<Vec<i32>>()}))?;
        let body = body_bytes(json!({
            "system": [{"type": "text", "text": compact}]
        }))?;
        assert!(matches!(
            measure_body(&body, &cfg(16)),
            CompressOutcome::Unchanged
        ));
        Ok(())
    }
}
