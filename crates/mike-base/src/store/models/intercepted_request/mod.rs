use bytes::Bytes;
use chrono::{DateTime, Utc};
use serde::{Serialize, Serializer};

use super::diagnostics::Diagnostic;
use super::request_id::RequestId;
use super::request_status::RequestStatus;
use super::{FailedStreamBlock, SystemPromptObservation, ToolResult};
use crate::types::{RequestParams, TokenUsage, ToolCall};

fn serialize_bytes_presence<S>(value: &Option<Bytes>, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_bool(value.is_some())
}

#[derive(Debug, Clone, Serialize)]
pub struct InterceptedRequest {
    pub id: RequestId,
    pub upstream_request_id: Option<String>,
    pub upstream_message_id: Option<String>,
    pub session_id: String,

    pub method: String,
    pub path: String,
    pub request_headers: Vec<(String, String)>,
    #[serde(
        rename = "has_request_body",
        serialize_with = "serialize_bytes_presence"
    )]
    pub request_body: Option<Bytes>,
    pub model: Option<String>,
    pub is_streaming: bool,
    pub message_count: usize,
    pub is_tool_loop: bool,
    pub last_user_message: String,
    pub tool_names: Vec<String>,
    pub params: RequestParams,
    pub project: Option<String>,
    pub git_repo: Option<String>,
    pub human_prompt: String,
    pub turn_type: String,
    pub turn_origin: String,
    pub subagent_persona: Option<String>,

    pub status: RequestStatus,
    pub response_status: Option<u16>,
    pub response_headers: Option<Vec<(String, String)>>,
    #[serde(
        rename = "has_response_body",
        serialize_with = "serialize_bytes_presence"
    )]
    pub response_body: Option<Bytes>,

    pub tool_calls: Vec<ToolCall>,
    pub token_usage: Option<TokenUsage>,
    pub stop_reason: Option<String>,
    pub stop_details: Option<serde_json::Value>,
    #[serde(skip)]
    pub message_fields: Option<serde_json::Value>,

    pub tool_results: Vec<ToolResult>,

    #[serde(skip)]
    pub tool_results_in_body: Vec<ToolResult>,

    #[serde(skip)]
    pub failed_stream_blocks: Vec<FailedStreamBlock>,

    #[serde(skip)]
    pub expected_stream_block_count: i32,

    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,

    #[serde(skip)]
    pub diagnostics: Vec<Diagnostic>,

    pub filter_applied: bool,
    #[serde(
        rename = "has_filtered_request_body",
        serialize_with = "serialize_bytes_presence"
    )]
    pub filtered_request_body: Option<Bytes>,

    #[serde(skip)]
    pub sent_body: Option<Bytes>,

    #[serde(skip)]
    pub transformation_manifest: Option<serde_json::Value>,

    #[serde(skip)]
    pub system_prompt: SystemPromptObservation,

    #[serde(default)]
    pub intercepted: bool,

    #[serde(default)]
    pub correction_request_id: Option<u64>,

    #[serde(default)]
    pub agent_instance_id: Option<i64>,

    #[serde(default)]
    pub claude_code_agent_id: Option<String>,
}

#[cfg(test)]
pub mod tests;
