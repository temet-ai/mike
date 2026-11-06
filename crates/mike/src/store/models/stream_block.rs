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
pub(crate) struct PersistedStreamBlockRow {
    pub(crate) seq_ord: i32,
    pub(crate) content: String,
    pub(crate) byte_size: i32,
    pub(crate) content_sha256: String,
    pub(crate) block_type: String,
    pub(crate) signature: Option<String>,
    pub(crate) wire_tool_use_id: Option<String>,
    pub(crate) arrived_at: DateTime<Utc>,
}

#[derive(Debug, Clone, serde::Serialize)]
pub(crate) struct StreamBlockDto {
    pub(crate) seq_ord: i32,
    pub(crate) content: String,
    pub(crate) byte_size: i32,
    pub(crate) content_sha256: String,
    pub(crate) block_type: String,
    pub(crate) signature: Option<String>,
    pub(crate) wire_tool_use_id: Option<String>,
    pub(crate) arrived_at: DateTime<Utc>,
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct RequestStreamBlocksPayload {
    pub(crate) request_id: u64,
    pub(crate) stream_blocks: Vec<StreamBlockDto>,
}
