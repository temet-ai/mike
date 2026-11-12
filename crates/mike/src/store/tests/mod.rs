#[cfg(test)]
mod attach_tool_results;
#[cfg(test)]
mod byte_budget;
#[cfg(test)]
mod composable_session;
#[cfg(test)]
mod diagnostic_stats;
#[cfg(test)]
mod entry_ops;
#[cfg(test)]
mod findings;
#[cfg(test)]
#[cfg(test)]
mod helpers;
#[cfg(test)]
mod memory_breakdown;
#[cfg(test)]
mod read_dedup;
#[cfg(test)]
mod reminders;
#[cfg(test)]
mod session_index;
#[cfg(test)]
mod session_lookup;
#[cfg(test)]
pub(crate) use helpers::*;

use super::models::{InterceptedRequest, RequestId, RequestStatus, SystemPromptObservation};
use crate::types::RequestParams;
use chrono::{DateTime, Utc};

pub fn make_test_request(id_override: Option<RequestId>, session_id: &str) -> InterceptedRequest {
    make_test_request_with_timestamp(id_override, session_id, Utc::now())
}

pub(crate) fn make_test_request_with_timestamp(
    id_override: Option<RequestId>,
    session_id: &str,
    created_at: DateTime<Utc>,
) -> InterceptedRequest {
    let id = id_override.unwrap_or_else(RequestId::next);
    InterceptedRequest {
        id,
        upstream_request_id: None,
        upstream_message_id: None,
        session_id: session_id.to_string(),
        method: "POST".to_string(),
        path: "/v1/messages".to_string(),
        request_headers: vec![],
        request_body: None,
        model: Some("claude-sonnet-4-20250514".to_string()),
        is_streaming: false,
        message_count: 1,
        is_tool_loop: false,
        last_user_message: "hello".to_string(),
        tool_names: vec![],
        params: RequestParams {
            max_tokens: Some(1024),
            temperature: None,
            thinking_enabled: false,
            thinking_budget: None,
            thinking_mode: None,
            effort: None,
        },
        project: None,
        git_repo: None,
        human_prompt: "hello".to_string(),
        turn_type: "user_prompt".to_string(),
        turn_origin: "primary".to_string(),
        subagent_persona: None,
        status: RequestStatus::Complete,
        response_status: Some(200),
        response_headers: None,
        response_body: None,
        tool_calls: vec![],
        token_usage: None,
        stop_reason: None,
        stop_details: None,
        message_fields: None,
        tool_results: Vec::new(),
        tool_results_in_body: Vec::new(),
        failed_stream_blocks: Vec::new(),
        expected_stream_block_count: 0,
        created_at,
        completed_at: None,
        diagnostics: Vec::new(),
        filter_applied: false,
        filtered_request_body: None,
        sent_body: None,
        transformation_manifest: None,
        system_prompt: SystemPromptObservation::default(),
        intercepted: false,
        correction_request_id: None,
        agent_instance_id: None,
        claude_code_agent_id: None,
    }
}
