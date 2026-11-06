use chrono::Utc;
use serde_json::json;

use crate::lesson_worker::LessonSnapshot;
use crate::store::models::{
    Diagnostic, DiagnosticCategory, InterceptedRequest, RequestId, RequestStatus,
    SystemPromptObservation,
};
use crate::types::RequestParams;

pub(super) fn sample_request(id: RequestId, session_id: &str) -> InterceptedRequest {
    InterceptedRequest {
        id,
        upstream_request_id: None,
        upstream_message_id: None,
        session_id: session_id.to_string(),
        method: "POST".to_string(),
        path: "/v1/messages".to_string(),
        request_headers: vec![],
        request_body: None,
        model: None,
        is_streaming: false,
        message_count: 0,
        is_tool_loop: false,
        last_user_message: String::new(),
        tool_names: vec![],
        params: RequestParams {
            max_tokens: None,
            temperature: None,
            thinking_enabled: false,
            thinking_budget: None,
            thinking_mode: None,
            effort: None,
        },
        project: None,
        git_repo: None,
        human_prompt: String::new(),
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
        created_at: Utc::now(),
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

pub(super) fn mk_finding(matched: &str, advice: &str, range_start: usize) -> Diagnostic {
    let len = matched.len();
    Diagnostic {
        category: DiagnosticCategory::Overselling,
        analyzer: "test".to_string(),
        advice: advice.to_string(),
        matched_text: matched.to_string(),
        match_range: (range_start, range_start + len),
        other_hits: 0,
        created_at: Utc::now(),
    }
}

pub(super) fn user_turn_body() -> anyhow::Result<Vec<u8>> {
    Ok(serde_json::to_vec(&json!({
        "model": "claude-sonnet-4",
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "follow-up"}]},
        ],
    }))?)
}

pub(super) fn empty_store() -> tokio::sync::RwLock<crate::store::RequestStore> {
    tokio::sync::RwLock::new(crate::store::RequestStore::new(
        crate::store::retention::RetentionPolicy::default(),
    ))
}

pub(super) fn noop_push_deps() -> crate::proxy::remote_control::inject::PushDeps {
    crate::proxy::remote_control::inject::PushDeps {
        persist: None,
        remote_control_streams: crate::proxy::remote_control::ReplyStreams::default(),
        gate: std::sync::Arc::new(crate::injection::InjectionGate::default()),
        store: std::sync::Arc::new(empty_store()),
    }
}

pub(super) fn subagent_body_with_system() -> anyhow::Result<Vec<u8>> {
    Ok(serde_json::to_vec(&json!({
        "model": "claude-sonnet-4",
        "system": [{"type": "text", "text": "You are a code reviewer."}],
        "messages": [
            {"role": "user", "content": [{"type": "text", "text": "please review this diff"}]},
        ],
    }))?)
}

pub(super) fn code_reviewer_snapshot() -> LessonSnapshot {
    LessonSnapshot::from_agent_fixtures(
        [("code-reviewer".to_string(), 7_i64)],
        [(
            7_i64,
            vec![(
                "correction".to_string(),
                "never approve without running tests".to_string(),
            )],
        )],
    )
}
