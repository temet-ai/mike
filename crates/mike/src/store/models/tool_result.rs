use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ToolResult {
    pub tool_use_id: String,
    pub is_error: bool,
    pub content_sha256: String,
    pub content_bytes: u64,
    pub content: serde_json::Value,
}
