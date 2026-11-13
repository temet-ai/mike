use bytes::Bytes;

use crate::state::AppState;
use crate::store::models::{InterceptedRequest, RequestId, RequestStatus};
use crate::wire::genuine::markers;

pub struct LaneIdentity {
    turn_origin: String,
    subagent_persona: Option<String>,
    claude_code_agent_id: Option<String>,
    agent_instance_id: Option<i64>,
}

impl LaneIdentity {
    fn main_lane() -> Self {
        Self {
            turn_origin: "primary".to_string(),
            subagent_persona: None,
            claude_code_agent_id: None,
            agent_instance_id: None,
        }
    }

    pub fn from_request(req: &InterceptedRequest) -> Self {
        let turn_origin = match req.turn_origin.as_str() {
            "primary_first" => "primary".to_string(),
            other => other.to_string(),
        };
        Self {
            turn_origin,
            subagent_persona: req.subagent_persona.clone(),
            claude_code_agent_id: req.claude_code_agent_id.clone(),
            agent_instance_id: req.agent_instance_id,
        }
    }

    pub async fn of_request(state: &AppState, id: RequestId) -> Self {
        state
            .store
            .read()
            .await
            .get(&id)
            .map_or_else(Self::main_lane, Self::from_request)
    }
}

pub fn proxy_tool_result_prompt(tool_name: &str) -> String {
    format!("{}{tool_name}'", markers::PROXY_TOOL_RESULT)
}

pub fn weasel_correction_prompt(trigger_word: &str) -> String {
    format!("{}{trigger_word}'", markers::WEASEL_CORRECTION)
}

pub fn make_correction_intercepted_request(
    id: RequestId,
    session_id: String,
    correction_body: Bytes,
    human_prompt: String,
    created_at: chrono::DateTime<chrono::Utc>,
    lane: LaneIdentity,
) -> InterceptedRequest {
    InterceptedRequest {
        id,
        upstream_request_id: None,
        upstream_message_id: None,
        session_id,
        method: "POST".to_string(),
        path: "/v1/messages".to_string(),
        request_headers: vec![],
        request_body: Some(correction_body),
        model: None,
        is_streaming: true,
        message_count: 0,
        is_tool_loop: false,
        last_user_message: String::new(),
        tool_names: vec![],
        params: crate::types::RequestParams {
            max_tokens: None,
            temperature: None,
            thinking_enabled: false,
            thinking_budget: None,
            thinking_mode: None,
            effort: None,
        },
        project: None,
        git_repo: None,
        human_prompt,
        turn_type: "tool_loop".to_string(),
        turn_origin: lane.turn_origin,
        subagent_persona: lane.subagent_persona,
        status: RequestStatus::Pending,
        response_status: None,
        response_headers: None,
        response_body: None,
        tool_calls: vec![],
        token_usage: None,
        stop_reason: None,
        stop_details: None,
        message_fields: None,
        tool_results: vec![],
        tool_results_in_body: vec![],
        failed_stream_blocks: vec![],
        expected_stream_block_count: 0,
        created_at,
        completed_at: None,
        diagnostics: vec![],
        filter_applied: false,
        filtered_request_body: None,
        sent_body: None,
        transformation_manifest: None,
        system_prompt: crate::store::models::SystemPromptObservation::default(),
        intercepted: false,
        correction_request_id: None,
        agent_instance_id: lane.agent_instance_id,
        claude_code_agent_id: lane.claude_code_agent_id,
    }
}

#[cfg(test)]
#[path = "support_tests.rs"]
pub mod support_tests;
