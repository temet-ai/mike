use serde_json::json;

use super::*;
use crate::store::retention::RetentionPolicy;
use crate::types::ToolCall;

fn sample_params() -> RequestParams {
    RequestParams {
        max_tokens: Some(1024),
        temperature: None,
        thinking_enabled: false,
        thinking_budget: None,
        thinking_mode: None,
        effort: None,
    }
}

fn sample_inputs(id: RequestId, session_id: &str) -> InterceptedRequestInputs {
    InterceptedRequestInputs {
        id,
        session_id: session_id.to_string(),
        method: "POST".to_string(),
        path: "/v1/messages".to_string(),
        request_headers: vec![("x-test".to_string(), "1".to_string())],
        request_body: Some(Bytes::from_static(b"{}")),
        model: Some("claude-x".to_string()),
        is_streaming: true,
        message_count: 3,
        is_tool_loop: false,
        last_user_message: "hi".to_string(),
        tool_names: vec!["Bash".to_string()],
        params: sample_params(),
        project: Some("projects/temet/temet-proxy".to_string()),
        git_repo: Some("temet-proxy".to_string()),
        human_prompt: "hi".to_string(),
        turn_type: "user_prompt".to_string(),
        turn_origin: "primary".to_string(),
        subagent_persona: None,
        tool_results_in_body: Vec::new(),
        filter_applied: false,
        sent_body: None,
        transformation_manifest: None,
        system_prompt: SystemPromptObservation::default(),
        claude_code_agent_id: None,
    }
}

#[test]
fn build_intercepted_request_carries_inputs_through_and_fixes_response_state() {
    let req = build_intercepted_request(sample_inputs(RequestId::next(), "session-a"));
    assert_eq!(req.session_id, "session-a");
    assert_eq!(req.model.as_deref(), Some("claude-x"));
    assert_eq!(req.message_count, 3);
    assert!(req.is_streaming);
    assert_eq!(req.project.as_deref(), Some("projects/temet/temet-proxy"));

    assert_eq!(req.status, RequestStatus::Pending);
    assert!(req.response_status.is_none());
    assert!(req.tool_calls.is_empty());
    assert!(req.tool_results.is_empty());
    assert!(!req.intercepted);
    assert!(req.correction_request_id.is_none());
    assert!(req.completed_at.is_none());
}

#[test]
fn build_system_prompt_observation_drops_blocks_when_unknown_blocks_present() {
    let obs = build_system_prompt_observation(SystemPromptObservationInputs {
        prompt_sha256: None,
        system_blocks_value: Some(json!([{"type": "text", "text": "hi"}])),
        composition_id: None,
        composition_applied: false,
        unknown_blocks: Some(json!({"reason": "unrecognized shape"})),
        filter_modified: true,
        firing_rule_ids: vec![1, 2],
    });
    assert!(
        obs.blocks.is_none(),
        "unknown shape has no canonical blocks"
    );
    assert!(obs.unknown_blocks.is_some());
    assert!(obs.filter_modified);
    assert_eq!(obs.firing_rule_ids, vec![1, 2]);
}

#[test]
fn build_system_prompt_observation_keeps_blocks_when_classified() {
    let blocks = json!([{"type": "text", "text": "hi"}]);
    let obs = build_system_prompt_observation(SystemPromptObservationInputs {
        prompt_sha256: Some("deadbeef".to_string()),
        system_blocks_value: Some(blocks.clone()),
        composition_id: Some(7),
        composition_applied: true,
        unknown_blocks: None,
        filter_modified: false,
        firing_rule_ids: Vec::new(),
    });
    assert_eq!(obs.blocks, Some(blocks));
    assert_eq!(obs.composition_id, Some(7));
    assert!(obs.composition_applied);
}

#[test]
fn extract_body_tool_results_finds_blocks_in_messages() {
    let body = json!({
        "messages": [
            {"role": "user", "content": [
                {"type": "tool_result", "tool_use_id": "t1", "content": "ok"}
            ]}
        ]
    });
    let results = extract_body_tool_results(Some(&body));
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].tool_use_id, "t1");
}

#[test]
fn extract_body_tool_results_empty_when_no_body() {
    assert!(extract_body_tool_results(None).is_empty());
}

#[tokio::test]
async fn insert_stub_and_mint_id_keeps_process_local_id_without_persistence() -> anyhow::Result<()>
{
    let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default())));
    let id = RequestId::next();
    let mut req = build_intercepted_request(sample_inputs(id, "session-b"));
    let minted = insert_stub_and_mint_id(&store, &mut req, true).await;
    assert_eq!(minted, id);
    assert_eq!(req.id, id);
    Ok(())
}

#[tokio::test]
async fn insert_and_attach_tool_results_matches_a_later_tool_result_to_the_emitting_request()
-> anyhow::Result<()> {
    let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default())));
    let session_id = "session-c";

    let emitter_id = RequestId::next();
    let mut emitter = build_intercepted_request(sample_inputs(emitter_id, session_id));
    emitter.tool_calls.push(ToolCall {
        tool_id: "tool_1".to_string(),
        name: "Bash".to_string(),
        input_json: "{}".to_string(),
        complete: true,
    });
    let outcome = insert_and_attach_tool_results(&store, emitter, session_id, Vec::new()).await;
    assert!(outcome.touched.is_empty());
    assert!(outcome.db_fallback_candidates.is_empty());

    let carrier_id = RequestId::next();
    let carrier = build_intercepted_request(sample_inputs(carrier_id, session_id));
    let carried_result = ToolResult {
        tool_use_id: "tool_1".to_string(),
        is_error: false,
        content_sha256: "sha".to_string(),
        content_bytes: 2,
        content: json!("ok"),
    };
    let outcome =
        insert_and_attach_tool_results(&store, carrier, session_id, vec![carried_result]).await;
    assert_eq!(outcome.touched, vec![emitter_id]);
    assert!(outcome.db_fallback_candidates.is_empty());
    Ok(())
}

#[tokio::test]
async fn insert_and_attach_tool_results_returns_db_fallback_when_session_bucket_is_empty()
-> anyhow::Result<()> {
    let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default())));
    let result = ToolResult {
        tool_use_id: "orphan".to_string(),
        is_error: false,
        content_sha256: "sha".to_string(),
        content_bytes: 1,
        content: json!("ok"),
    };
    let outcome = insert_and_attach_tool_results(
        &store,
        build_intercepted_request(sample_inputs(RequestId::next(), "session-restart")),
        "a-different-session-with-no-bucket",
        vec![result],
    )
    .await;
    assert!(outcome.touched.is_empty());
    assert_eq!(outcome.db_fallback_candidates.len(), 1);
    Ok(())
}

fn test_event_tx() -> broadcast::Sender<ProxyEvent> {
    broadcast::channel(16).0
}

#[test]
fn register_in_flight_returns_a_registered_guard_under_capacity() {
    let in_flight = Arc::new(InFlightRegistry::with_capacity(4));
    let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default())));
    let event_tx = test_event_tx();
    let request_id = RequestId::next();

    match register_in_flight(&in_flight, &store, &event_tx, request_id, "session-d") {
        InFlightRegistration::Registered(guard) => {
            guard.forget();
        }
        InFlightRegistration::CapacityExceeded { .. } => {
            panic!("expected registration to succeed under capacity")
        }
    }
    assert!(in_flight.get(request_id).is_some());
}

#[test]
fn register_in_flight_returns_capacity_exceeded_when_saturated() {
    let in_flight = Arc::new(InFlightRegistry::with_capacity(1));
    let store = Arc::new(RwLock::new(RequestStore::new(RetentionPolicy::default())));
    let event_tx = test_event_tx();

    let first = RequestId::next();
    match register_in_flight(&in_flight, &store, &event_tx, first, "session-e") {
        InFlightRegistration::Registered(guard) => guard.forget(),
        InFlightRegistration::CapacityExceeded { .. } => {
            panic!("first registration must succeed")
        }
    }

    let second = RequestId::next();
    match register_in_flight(&in_flight, &store, &event_tx, second, "session-e") {
        InFlightRegistration::Registered(_) => {
            panic!("second registration must be rejected — registry is at capacity")
        }
        InFlightRegistration::CapacityExceeded { max } => {
            assert_eq!(max, 1);
        }
    }
}
