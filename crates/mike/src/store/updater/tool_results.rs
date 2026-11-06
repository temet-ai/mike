use sha2::{Digest, Sha256};

use crate::store::models::ToolResult;

pub(crate) fn extract_tool_results_from_body(body: &serde_json::Value) -> Vec<ToolResult> {
    let Some(messages) = body.get("messages").and_then(|m| m.as_array()) else {
        return Vec::new();
    };
    messages
        .iter()
        .filter_map(|msg| msg.get("content").and_then(|c| c.as_array()))
        .flatten()
        .filter_map(extract_tool_result_block)
        .collect()
}

fn extract_tool_result_block(block: &serde_json::Value) -> Option<ToolResult> {
    let block_type = block.get("type").and_then(|t| t.as_str())?;
    if block_type != "tool_result" {
        return None;
    }
    let Some(tool_use_id) = block.get("tool_use_id").and_then(|t| t.as_str()) else {
        tracing::warn!("tool_result block missing tool_use_id; skipping");
        return None;
    };
    let is_error = block
        .get("is_error")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let content = block
        .get("content")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let (content_sha256, content_bytes) = hash_tool_result_content(tool_use_id, &content)?;

    Some(ToolResult {
        tool_use_id: tool_use_id.to_string(),
        is_error,
        content_sha256,
        content_bytes,
        content,
    })
}

fn hash_tool_result_content(
    tool_use_id: &str,
    content: &serde_json::Value,
) -> Option<(String, u64)> {
    let bytes = match serde_json::to_vec(content) {
        Ok(b) => b,
        Err(e) => {
            tracing::warn!(
                tool_use_id = %tool_use_id,
                error = %e,
                "failed to serialize tool_result content for sha; skipping"
            );
            return None;
        }
    };
    let mut hasher = Sha256::new();
    hasher.update(&bytes);
    let content_sha256 = hex::encode(hasher.finalize());
    let content_bytes = bytes.len() as u64;
    Some((content_sha256, content_bytes))
}

#[cfg(test)]
mod tests;
