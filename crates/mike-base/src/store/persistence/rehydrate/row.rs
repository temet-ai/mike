use chrono::{DateTime, Utc};
use serde::Deserialize;

use crate::store::models::{InterceptedRequest, RequestId, RequestStatus};
use crate::store::persistence::PersistError;
use crate::types::{RequestParams, TokenUsage, ToolCall};

#[derive(Debug, Clone, Deserialize)]
struct RequestParamsDto {
    max_tokens: Option<u32>,
    temperature: Option<f32>,
    thinking_enabled: bool,
    thinking_budget: Option<u32>,
    thinking_mode: Option<String>,
    #[serde(default)]
    effort: Option<String>,
}

impl From<RequestParamsDto> for RequestParams {
    fn from(d: RequestParamsDto) -> Self {
        Self {
            max_tokens: d.max_tokens,
            temperature: d.temperature,
            thinking_enabled: d.thinking_enabled,
            thinking_budget: d.thinking_budget,
            thinking_mode: d.thinking_mode,
            effort: d.effort,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
struct TokenUsageDto {
    input_tokens: u32,
    output_tokens: u32,
    cache_creation_input_tokens: Option<u32>,
    cache_read_input_tokens: Option<u32>,
}

impl From<TokenUsageDto> for TokenUsage {
    fn from(d: TokenUsageDto) -> Self {
        Self {
            input_tokens: d.input_tokens,
            output_tokens: d.output_tokens,
            cache_creation_input_tokens: d.cache_creation_input_tokens,
            cache_read_input_tokens: d.cache_read_input_tokens,
        }
    }
}

pub fn parse_status(raw: &str) -> RequestStatus {
    match raw {
        "pending" | "Pending" => RequestStatus::Pending,
        "streaming" | "Streaming" => RequestStatus::Streaming,
        "complete" | "Complete" => RequestStatus::Complete,
        other => match other.strip_prefix("Error: ") {
            Some(msg) => RequestStatus::Error(msg.to_string()),
            None => RequestStatus::Error(other.to_string()),
        },
    }
}

pub fn normalize_status_code(raw: &str) -> String {
    match raw {
        "pending" => "Pending".to_string(),
        "streaming" => "Streaming".to_string(),
        "complete" => "Complete".to_string(),
        "error" => "Error".to_string(),
        other => other.to_string(),
    }
}

#[derive(Debug, sqlx::FromRow)]
pub struct PersistedRequestRow {
    pub id: i64,
    pub session_id: String,
    pub method: String,
    pub path: String,
    pub model: Option<String>,
    pub is_streaming: bool,
    pub message_count: i32,
    pub is_tool_loop: bool,
    pub status: String,
    pub request_headers: Option<serde_json::Value>,
    pub params: serde_json::Value,
    pub request_body: Option<Vec<u8>>,
    pub filtered_request_body: Option<Vec<u8>>,
    pub last_user_message: String,
    pub human_prompt: String,
    pub turn_type: String,
    pub turn_origin: String,
    pub subagent_persona: Option<String>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
    pub rsp_prompt_sha256: Option<String>,
    pub rsp_composition_id: Option<i64>,
    pub rsp_composition_applied: Option<bool>,
    pub rsp_unknown_blocks: Option<serde_json::Value>,
    pub rsp_filter_modified: Option<bool>,
    pub rsp_firing_rule_ids: Option<Vec<i64>>,
    pub sp_blocks: Option<serde_json::Value>,
    pub sp_blocks_source: String,
    pub project_path: Option<String>,

    pub rr_upstream_request_id: Option<String>,
    pub rr_upstream_message_id: Option<String>,
    pub rr_response_status: Option<i16>,
    pub rr_response_headers: Option<serde_json::Value>,
    pub rr_response_body: Option<Vec<u8>>,
    pub rr_token_usage: Option<serde_json::Value>,
    pub rr_stop_reason: Option<String>,
    pub rr_stop_details: Option<serde_json::Value>,
    pub intercepted: bool,
    pub correction_request_id: Option<i64>,
    pub agent_instance_id: Option<i64>,
    pub claude_code_agent_id: Option<String>,
    pub transformation_manifest: Option<serde_json::Value>,
}

pub fn parse_json_column<T, D>(column: &str, value: &serde_json::Value) -> Result<T, PersistError>
where
    D: serde::de::DeserializeOwned + Into<T>,
{
    let dto: D = serde_json::from_value(value.clone()).map_err(|e| {
        PersistError::DataInvariant(format!("failed to decode {column} during rehydrate: {e}"))
    })?;
    Ok(dto.into())
}

fn decode_request_headers(
    value: Option<serde_json::Value>,
) -> Result<Vec<(String, String)>, PersistError> {
    match value {
        Some(v) => serde_json::from_value(v).map_err(|e| {
            PersistError::DataInvariant(format!(
                "failed to decode request_headers during rehydrate: {e}"
            ))
        }),
        None => Ok(Vec::new()),
    }
}

fn decode_response_headers(
    value: Option<serde_json::Value>,
) -> Result<Option<Vec<(String, String)>>, PersistError> {
    match value {
        Some(v) if !v.is_null() => Ok(Some(serde_json::from_value(v).map_err(|e| {
            PersistError::DataInvariant(format!(
                "failed to decode response_headers during rehydrate: {e}"
            ))
        })?)),
        _ => Ok(None),
    }
}

fn decode_token_usage(
    value: Option<serde_json::Value>,
) -> Result<Option<TokenUsage>, PersistError> {
    match value {
        Some(v) if !v.is_null() && v.as_object().map(|o| !o.is_empty()).unwrap_or(true) => {
            Ok(Some(parse_json_column::<TokenUsage, TokenUsageDto>(
                "token_usage",
                &v,
            )?))
        }
        _ => Ok(None),
    }
}

fn decode_response_body(body: Option<Vec<u8>>) -> Option<bytes::Bytes> {
    let body = body?;
    if body.is_empty() {
        None
    } else {
        Some(bytes::Bytes::from(body))
    }
}

fn resolve_request_id(id: i64) -> Result<u64, PersistError> {
    if id < 0 {
        return Err(PersistError::DataInvariant(format!(
            "requests.id must be non-negative, got {id}"
        )));
    }
    Ok(id as u64)
}

fn decode_response_status(status: Option<i16>) -> Result<Option<u16>, PersistError> {
    match status {
        Some(s) => {
            if !(100..=599).contains(&s) {
                return Err(PersistError::DataInvariant(format!(
                    "response_status {s} out of u16 range"
                )));
            }
            Ok(Some(s as u16))
        }
        None => Ok(None),
    }
}

fn clamp_message_count(n: i32) -> usize {
    if n < 0 { 0 } else { n as usize }
}

fn system_prompt_observation(
    row: &mut PersistedRequestRow,
) -> crate::store::models::SystemPromptObservation {
    crate::store::models::SystemPromptObservation {
        prompt_sha256: row.rsp_prompt_sha256.take(),
        blocks: row.sp_blocks.take(),
        composition_id: row.rsp_composition_id,
        composition_applied: row.rsp_composition_applied.unwrap_or(false),
        unknown_blocks: row.rsp_unknown_blocks.take(),
        filter_modified: row.rsp_filter_modified.unwrap_or(false),
        firing_rule_ids: row.rsp_firing_rule_ids.take().unwrap_or_default(),
        source: crate::store::models::SystemPromptSource::from_db_code(&row.sp_blocks_source),
    }
}

struct DecodedRequestFields {
    request_headers: Vec<(String, String)>,
    response_headers: Option<Vec<(String, String)>>,
    params: RequestParams,
    token_usage: Option<TokenUsage>,
    request_body: Option<bytes::Bytes>,
    response_body: Option<bytes::Bytes>,
    response_status: Option<u16>,
    request_id: u64,
    message_count: usize,
    system_prompt: crate::store::models::SystemPromptObservation,
}

pub async fn row_to_intercepted(
    mut row: PersistedRequestRow,
    tool_names: Vec<String>,
    tool_calls: Vec<ToolCall>,
) -> Result<InterceptedRequest, PersistError> {
    let decoded = DecodedRequestFields {
        request_headers: decode_request_headers(row.request_headers.take())?,
        response_headers: decode_response_headers(row.rr_response_headers.take())?,
        params: parse_json_column::<RequestParams, RequestParamsDto>("params", &row.params)?,
        token_usage: decode_token_usage(row.rr_token_usage.take())?,
        request_body: row.request_body.take().map(bytes::Bytes::from),
        response_body: decode_response_body(row.rr_response_body.take()),
        response_status: decode_response_status(row.rr_response_status.take())?,
        request_id: resolve_request_id(row.id)?,
        message_count: clamp_message_count(row.message_count),
        system_prompt: system_prompt_observation(&mut row),
    };
    build_intercepted_request(row, tool_names, tool_calls, decoded)
}

fn build_intercepted_request(
    row: PersistedRequestRow,
    tool_names: Vec<String>,
    tool_calls: Vec<ToolCall>,
    decoded: DecodedRequestFields,
) -> Result<InterceptedRequest, PersistError> {
    let diagnostics: Vec<crate::store::models::Diagnostic> = Vec::new();

    Ok(InterceptedRequest {
        id: RequestId::from_raw(decoded.request_id),
        upstream_request_id: row.rr_upstream_request_id,
        upstream_message_id: row.rr_upstream_message_id,
        session_id: row.session_id,
        method: row.method,
        path: row.path,
        request_headers: decoded.request_headers,
        request_body: decoded.request_body,
        model: row.model,
        is_streaming: row.is_streaming,
        message_count: decoded.message_count,
        is_tool_loop: row.is_tool_loop,
        last_user_message: row.last_user_message,
        tool_names,
        params: decoded.params,
        project: row
            .project_path
            .filter(|p| p != crate::store::persistence::SENTINEL_PROJECT_PATH),
        git_repo: None,
        human_prompt: row.human_prompt,
        turn_type: row.turn_type,
        turn_origin: row.turn_origin,
        subagent_persona: row.subagent_persona,
        status: parse_status(&row.status),
        response_status: decoded.response_status,
        response_headers: decoded.response_headers,
        response_body: decoded.response_body,
        tool_calls,
        token_usage: decoded.token_usage,
        stop_reason: row.rr_stop_reason,
        stop_details: row.rr_stop_details,
        message_fields: None,
        tool_results: Vec::new(),
        tool_results_in_body: Vec::new(),
        failed_stream_blocks: Vec::new(),
        expected_stream_block_count: 0,
        created_at: row.created_at,
        completed_at: row.completed_at,
        diagnostics,
        filter_applied: row.filtered_request_body.is_some(),
        filtered_request_body: row.filtered_request_body.map(bytes::Bytes::from),
        sent_body: None,
        transformation_manifest: row.transformation_manifest,
        intercepted: row.intercepted,
        correction_request_id: row.correction_request_id.map(|v| v as u64),
        agent_instance_id: row.agent_instance_id,
        claude_code_agent_id: row.claude_code_agent_id,
        system_prompt: decoded.system_prompt,
    })
}
