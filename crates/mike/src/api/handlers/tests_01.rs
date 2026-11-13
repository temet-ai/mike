use super::*;

pub(crate) fn fixed_time(year: i32, month: u32, day: u32, hour: u32, min: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(year, month, day, hour, min, 0)
        .single()
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH)
}

#[test]
pub(crate) fn mask_openrouter_key_long_key_shows_prefix_and_suffix_only() {
    let masked = mask_openrouter_key("sk-or-v1-0123456789abcdef");
    assert_eq!(masked, "sk-or-v1-012...def");
    assert!(!masked.contains("3456789abcdef"), "masked: {masked}");
}

#[test]
pub(crate) fn mask_openrouter_key_short_key_shows_suffix_only() {
    let masked = mask_openrouter_key("sk-short");
    assert_eq!(masked, "...ort");
    assert!(!masked.contains("sk-sh"), "masked: {masked}");
}

pub(crate) fn empty_composer_cache()
-> HashMap<crate::compose_prompt::PromptClassKey, crate::wire::composition::Composition> {
    HashMap::new()
}

pub(crate) fn fixture_composition(
    prompt_sha256: &str,
    block_texts: &[&str],
) -> crate::wire::composition::Composition {
    use crate::wire::composition::{Composition, CompositionMode};
    let ts = fixed_time(2026, 4, 11, 12, 0);
    Composition {
        id: 0,
        uuid: uuid::Uuid::nil(),
        prompt_sha256: prompt_sha256.to_string(),
        label: "fixture-comp".to_string(),
        blocks: block_texts
            .iter()
            .map(|t| serde_json::json!({"type": "text", "text": t}))
            .collect(),
        mode: CompositionMode::Observe,
        lane: "main".to_string(),
        notes: None,
        agent_id: None,
        agent_name: None,
        agent_definition_path: None,
        version: 7,
        created_at: ts,
        updated_at: ts,
    }
}

pub(crate) fn observation_with_blocks(
    prompt_sha256: &str,
    block_texts: &[&str],
) -> crate::store::models::SystemPromptObservation {
    let blocks_json = Value::Array(
        block_texts
            .iter()
            .map(|t| serde_json::json!({"type": "text", "text": t}))
            .collect(),
    );
    crate::store::models::SystemPromptObservation {
        prompt_sha256: Some(prompt_sha256.to_string()),
        blocks: Some(blocks_json),
        composition_id: None,
        composition_applied: false,
        unknown_blocks: None,
        filter_modified: false,
        firing_rule_ids: Vec::new(),
        source: crate::store::models::SystemPromptSource::Exact,
    }
}

pub(crate) fn sample_request() -> InterceptedRequest {
    InterceptedRequest {
        id: RequestId::from_raw(42),
        upstream_request_id: None,
        upstream_message_id: None,
        session_id: "sess-abc".to_string(),
        method: "POST".to_string(),
        path: "/v1/messages".to_string(),
        request_headers: vec![],
        request_body: None,
        model: Some("claude-sonnet-4-20250514".to_string()),
        is_streaming: true,
        message_count: 3,
        is_tool_loop: false,
        last_user_message: String::new(),
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
        human_prompt: "Do the thing".to_string(),
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
        created_at: fixed_time(2026, 4, 11, 12, 0),
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
    }
}

#[tokio::test]
pub(crate) async fn get_request_transformation_manifest_returns_full_manifest_when_present()
-> anyhow::Result<()> {
    let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let mut req = sample_request();
    req.id = RequestId::from_raw(11);
    let manifest = serde_json::json!({
        "schema_version": 1,
        "original_sha256": "a".repeat(64),
        "sent_sha256": "b".repeat(64),
        "stages": [
            {
                "ordinal": 1,
                "stage": "filter",
                "outcome": "applied",
                "policy_id": "filter",
                "policy_version": 1,
                "resolved_parameters": {},
                "input_sha256": "a".repeat(64),
                "output_sha256": "b".repeat(64),
                "targets": [],
                "reason_code": "filter_rule_matched",
                "detail": null,
            },
        ],
    });
    req.transformation_manifest = Some(manifest);
    {
        let mut store = state.store.write().await;
        store.insert(req);
    }
    let Json(payload) = get_request_transformation_manifest(
        State(state),
        Path(11),
        Query(BodyPointerParams::default()),
    )
    .await
    .map_err(|s| anyhow::anyhow!("handler returned {s}"))?;
    assert_eq!(payload.request_id, 11);
    assert!(payload.has_manifest);
    let got = payload
        .manifest
        .ok_or_else(|| anyhow::anyhow!("manifest must be Some when has_manifest is true"))?;
    assert_eq!(got["schema_version"], 1);
    assert_eq!(
        got["stages"].as_array().map(Vec::len),
        Some(1),
        "full stages array must round-trip, not a sliced projection"
    );
    assert_eq!(got["stages"][0]["stage"], "filter");
    Ok(())
}

#[tokio::test]
pub(crate) async fn get_request_transformation_manifest_has_manifest_false_when_absent()
-> anyhow::Result<()> {
    let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let mut req = sample_request();
    req.id = RequestId::from_raw(12);
    req.transformation_manifest = None;
    {
        let mut store = state.store.write().await;
        store.insert(req);
    }
    let Json(payload) = get_request_transformation_manifest(
        State(state),
        Path(12),
        Query(BodyPointerParams::default()),
    )
    .await
    .map_err(|s| anyhow::anyhow!("handler returned {s}"))?;
    assert_eq!(payload.request_id, 12);
    assert!(!payload.has_manifest);
    assert!(payload.manifest.is_none());
    Ok(())
}

#[tokio::test]
pub(crate) async fn get_request_transformation_manifest_unknown_id_is_404() -> anyhow::Result<()> {
    let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let err = get_request_transformation_manifest(
        State(state),
        Path(999_999),
        Query(BodyPointerParams::default()),
    )
    .await
    .err()
    .ok_or_else(|| anyhow::anyhow!("expected 404 for an unknown id"))?;
    assert_eq!(err, StatusCode::NOT_FOUND);
    Ok(())
}

#[tokio::test]
pub(crate) async fn get_request_transformation_manifest_pointer_projects_full_subtree()
-> anyhow::Result<()> {
    let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let mut req = sample_request();
    req.id = RequestId::from_raw(13);
    req.transformation_manifest = Some(serde_json::json!({
        "schema_version": 1,
        "original_sha256": "a".repeat(64),
        "sent_sha256": "a".repeat(64),
        "stages": [],
    }));
    {
        let mut store = state.store.write().await;
        store.insert(req);
    }
    let Json(payload) = get_request_transformation_manifest(
        State(state),
        Path(13),
        Query(BodyPointerParams {
            pointer: Some("/schema_version".to_string()),
            ..BodyPointerParams::default()
        }),
    )
    .await
    .map_err(|s| anyhow::anyhow!("handler returned {s}"))?;
    assert!(payload.has_manifest);
    assert_eq!(payload.pointer.as_deref(), Some("/schema_version"));
    assert_eq!(payload.pointer_resolved, Some(true));
    assert_eq!(payload.manifest, Some(serde_json::json!(1)));
    Ok(())
}

#[tokio::test]
pub(crate) async fn get_request_body_index_lists_transformation_manifest_route()
-> anyhow::Result<()> {
    let state = AppState::new(crate::config::ProxyConfig::for_tests_in_memory()).await?;
    let mut req = sample_request();
    req.id = RequestId::from_raw(14);
    {
        let mut store = state.store.write().await;
        store.insert(req);
    }
    let Json(payload) = get_request_body_index(State(state), Path(14))
        .await
        .map_err(|s| anyhow::anyhow!("handler returned {s}"))?;
    assert_eq!(
        payload.transformation_manifest.href,
        "/api/requests/14/manifest"
    );
    assert!(payload.transformation_manifest.supports_pointer);
    Ok(())
}

#[test]
pub(crate) fn summary_from_copies_identity_and_metadata() {
    let req = sample_request();
    let summary = RequestSummary::from(&req);

    assert_eq!(summary.id, 42);
    assert_eq!(summary.method, "POST");
    assert_eq!(summary.path, "/v1/messages");
    assert_eq!(summary.model.as_deref(), Some("claude-sonnet-4-20250514"));
    assert_eq!(summary.status, "Complete");
    assert!(summary.is_streaming);
    assert_eq!(summary.message_count, 3);
}

#[test]
pub(crate) fn summary_from_propagates_max_tokens_from_params() {
    let req = sample_request();
    let summary = RequestSummary::from(&req);
    assert_eq!(summary.max_tokens, Some(1024));
}

#[test]
pub(crate) fn summary_from_renders_created_at_as_rfc3339() {
    let req = sample_request();
    let summary = RequestSummary::from(&req);
    assert!(
        summary.created_at.starts_with("2026-04-11T12:00:00"),
        "unexpected created_at: {}",
        summary.created_at
    );
}

#[test]
pub(crate) fn summary_from_pending_status_renders_as_string() {
    let mut req = sample_request();
    req.status = RequestStatus::Pending;
    assert_eq!(RequestSummary::from(&req).status, "Pending");
}

#[test]
pub(crate) fn summary_from_streaming_status_renders_as_string() {
    let mut req = sample_request();
    req.status = RequestStatus::Streaming;
    assert_eq!(RequestSummary::from(&req).status, "Streaming");
}

#[test]
pub(crate) fn summary_from_error_status_includes_error_message() {
    let mut req = sample_request();
    req.status = RequestStatus::Error("upstream 500".to_string());
    assert_eq!(
        RequestSummary::from(&req).status,
        "Error: upstream 500",
        "RequestStatus::Error Display format should match to_string"
    );
}

#[test]
pub(crate) fn summary_from_completed_at_when_set_emits_rfc3339() -> anyhow::Result<()> {
    let mut req = sample_request();
    req.completed_at = Some(fixed_time(2026, 4, 11, 12, 5));
    let summary = RequestSummary::from(&req);
    let completed = summary
        .completed_at
        .as_deref()
        .ok_or_else(|| anyhow::anyhow!("completed_at should be Some"))?;
    assert!(completed.starts_with("2026-04-11T12:05:00"));
    Ok(())
}

#[test]
pub(crate) fn summary_from_completed_at_when_none_emits_none() {
    let req = sample_request();
    assert!(RequestSummary::from(&req).completed_at.is_none());
}

#[test]
pub(crate) fn summary_from_includes_project_and_turn_type() {
    let mut req = sample_request();
    req.project = Some("ai/tools/proxy".to_string());
    let summary = RequestSummary::from(&req);
    assert_eq!(summary.project.as_deref(), Some("ai/tools/proxy"));
    assert_eq!(summary.human_prompt, "Do the thing");
    assert_eq!(summary.turn_type, "user_prompt");
}

#[test]
pub(crate) fn summary_from_human_prompt_passes_full_text_through() {
    let mut req = sample_request();
    req.human_prompt = "h".repeat(500);
    let summary = RequestSummary::from(&req);
    assert_eq!(
        summary.human_prompt.len(),
        500,
        "human_prompt on RequestSummary must NOT be truncated"
    );

    let detail = RequestDetail::from_request(&req, &empty_composer_cache(), None);
    assert_eq!(
        detail.human_prompt.len(),
        500,
        "human_prompt on RequestDetail must NOT be truncated"
    );
}

#[test]
pub(crate) fn summary_from_copies_tool_names_preserving_order() {
    let mut req = sample_request();
    req.tool_names = vec!["bash".to_string(), "read_file".to_string()];

    let summary = RequestSummary::from(&req);
    assert_eq!(
        summary.tool_names,
        vec!["bash".to_string(), "read_file".to_string()],
        "RequestSummary.tool_names must mirror InterceptedRequest.tool_names \
         in source order"
    );

    let detail = RequestDetail::from_request(&req, &empty_composer_cache(), None);
    assert_eq!(
        summary.tool_names, detail.tool_names,
        "list and detail projections must agree on tool_names"
    );
}

#[test]
pub(crate) fn summary_from_response_tool_count_and_tools_used() {
    let mut req = sample_request();
    req.tool_calls = vec![
        ToolCall {
            tool_id: "t1".to_string(),
            name: "bash".to_string(),
            input_json: "{}".to_string(),
            complete: true,
        },
        ToolCall {
            tool_id: "t2".to_string(),
            name: "bash".to_string(),
            input_json: "{}".to_string(),
            complete: true,
        },
        ToolCall {
            tool_id: "t3".to_string(),
            name: "read_file".to_string(),
            input_json: "{}".to_string(),
            complete: true,
        },
    ];
    let summary = RequestSummary::from(&req);
    assert_eq!(summary.response_tool_count, 3);
    assert_eq!(summary.response_tools_used, vec!["bash", "read_file"]);
}

#[test]
pub(crate) fn decode_utf8_none_input_yields_none() {
    assert!(decode_utf8(None).is_none());
}

#[test]
pub(crate) fn decode_utf8_valid_utf8_yields_string() {
    let bytes = Bytes::from_static(b"{\"model\":\"claude-sonnet-4-20250514\"}");
    assert_eq!(
        decode_utf8(Some(&bytes)).as_deref(),
        Some("{\"model\":\"claude-sonnet-4-20250514\"}")
    );
}

#[test]
pub(crate) fn decode_utf8_invalid_utf8_yields_none() {
    let bytes = Bytes::from_static(&[0xFF, 0xFE, 0x80]);
    assert!(decode_utf8(Some(&bytes)).is_none());
}
