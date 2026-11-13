use chrono::{DateTime, Utc};

#[derive(Debug, Clone)]
pub struct FailedStreamBlock {
    pub seq_ord: i32,
    pub block_type: String,
    pub content: String,
    pub signature: Option<String>,
    pub wire_tool_use_id: Option<String>,
    pub content_sha256: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct PersistedStreamBlockRow {
    pub seq_ord: i32,
    pub content: String,
    pub byte_size: i32,
    pub content_sha256: String,
    pub block_type: String,
    pub signature: Option<String>,
    pub wire_tool_use_id: Option<String>,
    pub arrived_at: DateTime<Utc>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct StreamBlockDto {
    pub seq_ord: i32,
    pub content: String,
    pub byte_size: i32,
    pub content_sha256: String,
    pub block_type: String,
    pub signature: Option<String>,
    pub wire_tool_use_id: Option<String>,
    pub arrived_at: DateTime<Utc>,
}

#[derive(Debug, serde::Serialize)]
pub struct RequestStreamBlocksPayload {
    pub request_id: u64,
    pub stream_blocks: Vec<StreamBlockDto>,
}
