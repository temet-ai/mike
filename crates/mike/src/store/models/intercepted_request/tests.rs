use super::*;
use crate::types::RequestParams;
use serde_json::Value;

fn sample_request() -> InterceptedRequest {
    InterceptedRequest {
        id: RequestId::from_raw(1),
        upstream_request_id: None,
        upstream_message_id: None,
        session_id: "sess-x".to_string(),
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
        created_at: chrono::Utc::now(),
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

#[test]
fn default_serialization_emits_has_body_booleans() -> anyhow::Result<()> {
    let mut req = sample_request();
    req.request_body = Some(Bytes::from("secret-payload"));
    req.response_body = None;

    let value: Value = serde_json::to_value(&req)?;

    assert_eq!(
        value.get("has_request_body"),
        Some(&Value::Bool(true)),
        "has_request_body must be true when request_body is Some"
    );
    assert_eq!(
        value.get("has_response_body"),
        Some(&Value::Bool(false)),
        "has_response_body must be false when response_body is None"
    );

    assert!(
        value.get("request_body").is_none(),
        "raw request_body field must not appear in default JSON"
    );
    assert!(
        value.get("response_body").is_none(),
        "raw response_body field must not appear in default JSON"
    );
    assert!(
        value.get("request_body_text").is_none(),
        "default serialization must not leak request_body_text"
    );
    assert!(
        value.get("response_body_text").is_none(),
        "default serialization must not leak response_body_text"
    );
    Ok(())
}

#[test]
fn default_serialization_does_not_leak_raw_bytes() -> anyhow::Result<()> {
    let mut req = sample_request();
    req.request_body = Some(Bytes::from("secret-payload"));
    req.response_body = Some(Bytes::from("other-secret"));

    let text = serde_json::to_string(&req)?;

    assert!(
        !text.contains("secret-payload"),
        "request body bytes leaked via default serialization: {text}"
    );
    assert!(
        !text.contains("other-secret"),
        "response body bytes leaked via default serialization: {text}"
    );

    let value: Value = serde_json::from_str(&text)?;
    assert_eq!(value.get("has_request_body"), Some(&Value::Bool(true)));
    assert_eq!(value.get("has_response_body"), Some(&Value::Bool(true)));
    Ok(())
}
