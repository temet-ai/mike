use super::*;
use crate::store::models::InterceptedRequest;
use crate::store::retention::RetentionPolicy;
use crate::types::{RequestParams, TokenUsage};
use std::time::Duration;

fn make_store_with_request() -> (Arc<RwLock<RequestStore>>, RequestId) {
    let policy = RetentionPolicy {
        max_entries: 10,
        max_age: Duration::from_secs(3600),
        prune_interval: Duration::from_secs(60),
        max_store_bytes: 0,
    };
    let mut store = RequestStore::new(policy);

    let id = RequestId::next();
    let req = InterceptedRequest {
        id,
        upstream_request_id: None,
        upstream_message_id: None,
        session_id: "sess-x".to_string(),
        method: "POST".to_string(),
        path: "/v1/messages".to_string(),
        request_headers: vec![],
        request_body: None,
        model: None,
        is_streaming: true,
        message_count: 1,
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
        status: RequestStatus::Streaming,
        response_status: None,
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
        system_prompt: crate::store::models::SystemPromptObservation::default(),
        intercepted: false,
        correction_request_id: None,
        agent_instance_id: None,
        claude_code_agent_id: None,
    };
    store.insert(req);
    (Arc::new(RwLock::new(store)), id)
}

#[tokio::test]
async fn apply_text_delta_is_not_terminal() -> anyhow::Result<()> {
    let (store, id) = make_store_with_request();
    let event = ProxyEvent::TextDelta {
        id,
        text: "hello ".to_string(),
    };
    assert!(
        !apply_event(&store, &event).await,
        "TextDelta should not be terminal"
    );
    Ok(())
}

#[tokio::test]
async fn apply_thinking_delta_is_not_terminal() -> anyhow::Result<()> {
    let (store, id) = make_store_with_request();
    let event = ProxyEvent::ThinkingDelta {
        id,
        thinking: "step 1".to_string(),
    };
    assert!(
        !apply_event(&store, &event).await,
        "ThinkingDelta should not be terminal"
    );
    Ok(())
}

#[tokio::test]
async fn apply_tool_use_start_then_input_then_end_builds_tool_call() -> anyhow::Result<()> {
    let (store, id) = make_store_with_request();

    apply_event(
        &store,
        &ProxyEvent::ToolUseStart {
            id,
            tool_id: "toolu_1".to_string(),
            tool_name: "bash".to_string(),
        },
    )
    .await;
    apply_event(
        &store,
        &ProxyEvent::ToolInputDelta {
            id,
            partial_json: "{\"cmd\":".to_string(),
        },
    )
    .await;
    apply_event(
        &store,
        &ProxyEvent::ToolInputDelta {
            id,
            partial_json: "\"ls\"}".to_string(),
        },
    )
    .await;
    apply_event(&store, &ProxyEvent::ToolUseEnd { id }).await;

    let s = store.read().await;
    let req = s
        .get(&id)
        .ok_or_else(|| anyhow::anyhow!("request vanished"))?;
    assert_eq!(req.tool_calls.len(), 1);
    let tool = req
        .tool_calls
        .first()
        .ok_or_else(|| anyhow::anyhow!("missing tool call"))?;
    assert_eq!(tool.tool_id, "toolu_1");
    assert_eq!(tool.name, "bash");
    assert_eq!(tool.input_json, "{\"cmd\":\"ls\"}");
    assert!(tool.complete);
    Ok(())
}

#[tokio::test]
async fn apply_usage_received_populates_token_usage() -> anyhow::Result<()> {
    let (store, id) = make_store_with_request();
    let event = ProxyEvent::UsageReceived {
        id,
        usage: TokenUsage {
            input_tokens: 100,
            output_tokens: 0,
            cache_creation_input_tokens: Some(50),
            cache_read_input_tokens: Some(25),
        },
    };
    apply_event(&store, &event).await;
    let s = store.read().await;
    let req = s
        .get(&id)
        .ok_or_else(|| anyhow::anyhow!("request vanished"))?;
    let usage = req
        .token_usage
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("usage should be set"))?;
    assert_eq!(usage.input_tokens, 100);
    assert_eq!(usage.cache_creation_input_tokens, Some(50));
    assert_eq!(usage.cache_read_input_tokens, Some(25));
    Ok(())
}

#[tokio::test]
async fn apply_response_complete_sets_terminal_status_and_merges_usage() -> anyhow::Result<()> {
    let (store, id) = make_store_with_request();

    apply_event(
        &store,
        &ProxyEvent::UsageReceived {
            id,
            usage: TokenUsage {
                input_tokens: 100,
                output_tokens: 0,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
            },
        },
    )
    .await;

    let terminal = apply_event(
        &store,
        &ProxyEvent::ResponseComplete {
            id,
            stop_reason: Some("end_turn".to_string()),
            stop_details: None,
            usage: Some(TokenUsage {
                input_tokens: 0,
                output_tokens: 42,
                cache_creation_input_tokens: None,
                cache_read_input_tokens: None,
            }),
        },
    )
    .await;
    assert!(terminal, "ResponseComplete must be terminal");

    let s = store.read().await;
    let req = s
        .get(&id)
        .ok_or_else(|| anyhow::anyhow!("request vanished"))?;
    assert!(matches!(req.status, RequestStatus::Complete));
    assert_eq!(req.stop_reason.as_deref(), Some("end_turn"));
    assert!(req.completed_at.is_some());
    let usage = req
        .token_usage
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("usage should survive"))?;
    assert_eq!(usage.input_tokens, 100, "input tokens must not be zeroed");
    assert_eq!(usage.output_tokens, 42);
    Ok(())
}

#[tokio::test]
async fn apply_response_error_sets_error_status_and_is_terminal() -> anyhow::Result<()> {
    let (store, id) = make_store_with_request();
    let terminal = apply_event(
        &store,
        &ProxyEvent::ResponseError {
            id,
            error: "overloaded_error".to_string(),
        },
    )
    .await;
    assert!(terminal);

    let s = store.read().await;
    let req = s
        .get(&id)
        .ok_or_else(|| anyhow::anyhow!("request vanished"))?;
    match &req.status {
        RequestStatus::Error(msg) => assert_eq!(msg, "overloaded_error"),
        other => anyhow::bail!("expected Error status, got {other:?}"),
    }
    assert!(req.completed_at.is_some());
    Ok(())
}

#[tokio::test]
async fn apply_model_confirmed_sets_model_field() -> anyhow::Result<()> {
    let (store, id) = make_store_with_request();
    apply_event(
        &store,
        &ProxyEvent::ModelConfirmed {
            id,
            model: "claude-opus-4-20250514".to_string(),
        },
    )
    .await;
    let s = store.read().await;
    let req = s
        .get(&id)
        .ok_or_else(|| anyhow::anyhow!("request vanished"))?;
    assert_eq!(req.model.as_deref(), Some("claude-opus-4-20250514"));
    Ok(())
}

#[tokio::test]
async fn apply_upstream_message_id_received_sets_field() -> anyhow::Result<()> {
    let (store, id) = make_store_with_request();
    apply_event(
        &store,
        &ProxyEvent::UpstreamMessageIdReceived {
            id,
            upstream_message_id: "msg_01ABC".to_string(),
        },
    )
    .await;
    let s = store.read().await;
    let req = s
        .get(&id)
        .ok_or_else(|| anyhow::anyhow!("request vanished"))?;
    assert_eq!(req.upstream_message_id.as_deref(), Some("msg_01ABC"));
    assert!(req.upstream_request_id.is_none());
    Ok(())
}

#[tokio::test]
async fn apply_upstream_message_id_received_empty_string_is_noop() -> anyhow::Result<()> {
    let (store, id) = make_store_with_request();
    apply_event(
        &store,
        &ProxyEvent::UpstreamMessageIdReceived {
            id,
            upstream_message_id: String::new(),
        },
    )
    .await;
    let s = store.read().await;
    let req = s
        .get(&id)
        .ok_or_else(|| anyhow::anyhow!("request vanished"))?;
    assert!(req.upstream_message_id.is_none());
    Ok(())
}

#[tokio::test]
async fn apply_model_confirmed_empty_string_is_noop() -> anyhow::Result<()> {
    let (store, id) = make_store_with_request();
    apply_event(
        &store,
        &ProxyEvent::ModelConfirmed {
            id,
            model: String::new(),
        },
    )
    .await;
    let s = store.read().await;
    let req = s
        .get(&id)
        .ok_or_else(|| anyhow::anyhow!("request vanished"))?;
    assert!(req.model.is_none());
    Ok(())
}

#[tokio::test]
async fn apply_events_for_unknown_id_are_silent() -> anyhow::Result<()> {
    let (store, _id) = make_store_with_request();
    let fake = RequestId::from_raw(999_999);
    let terminal = apply_event(
        &store,
        &ProxyEvent::TextDelta {
            id: fake,
            text: "lost".to_string(),
        },
    )
    .await;
    assert!(!terminal);
    Ok(())
}

#[tokio::test]
async fn apply_tool_use_start_over_cap_drops_new_calls() -> anyhow::Result<()> {
    let (store, id) = make_store_with_request();

    for i in 0..MAX_TOOL_CALLS {
        apply_event(
            &store,
            &ProxyEvent::ToolUseStart {
                id,
                tool_id: format!("t{i}"),
                tool_name: "bash".to_string(),
            },
        )
        .await;
    }
    apply_event(
        &store,
        &ProxyEvent::ToolUseStart {
            id,
            tool_id: "overflow".to_string(),
            tool_name: "bash".to_string(),
        },
    )
    .await;

    let s = store.read().await;
    let req = s
        .get(&id)
        .ok_or_else(|| anyhow::anyhow!("request vanished"))?;
    assert_eq!(req.tool_calls.len(), MAX_TOOL_CALLS);
    Ok(())
}

#[tokio::test]
async fn analysis_completed_after_run_with_no_findings_is_true() -> anyhow::Result<()> {
    let (store, id) = make_store_with_request();
    let registry = Arc::new(arc_swap::ArcSwap::from_pointee(
        crate::analysis::AnalyzerRegistry::with_defaults(
            &crate::analysis::AnalysisConfig::default(),
        )?,
    ));
    let (tx, _rx) = tokio::sync::broadcast::channel(8);
    store.write().await.set_analysis(registry, tx);

    anyhow::ensure!(!analysis_completed(&id), "pending before analysis runs");
    run_analysis_and_notify(&store, id, "plain text with nothing to flag").await;
    anyhow::ensure!(
        analysis_completed(&id),
        "completed after analysis even with no findings"
    );
    Ok(())
}
