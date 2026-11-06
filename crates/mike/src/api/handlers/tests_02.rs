use super::tests_01::*;
use super::*;

#[test]
pub(crate) fn request_detail_omits_body_text_and_has_body_fields() -> anyhow::Result<()> {
    let mut req = sample_request();
    let payload = "{\"messages\":[{\"role\":\"user\",\"content\":\"hi\"}]}";
    req.request_body = Some(Bytes::from(payload));
    req.response_body = None;

    let detail = RequestDetail::from_request(&req, &empty_composer_cache(), None);
    let value = serde_json::to_value(&detail)?;

    for forbidden in [
        "request_body_text",
        "response_body_text",
        "has_request_body",
        "has_response_body",
        "body_truncated",
        "request_body_truncated",
        "response_body_truncated",
        "text_truncated",
        "thinking_truncated",
        "tool_input_truncated",
    ] {
        assert!(
            value.get(forbidden).is_none(),
            "RequestDetail must not emit `{forbidden}`: {value}"
        );
    }

    assert_eq!(
        value.get("request_body_size"),
        Some(&Value::from(payload.len())),
        "request_body_size must reflect the captured byte count"
    );
    assert_eq!(
        value.get("response_body_size"),
        Some(&Value::from(0u64)),
        "response_body_size must be 0 when response body is absent"
    );
    Ok(())
}

#[test]
pub(crate) fn system_blocks_projection_cache_miss_yields_null_match_and_version()
-> anyhow::Result<()> {
    let mut req = sample_request();
    req.system_prompt = observation_with_blocks(
        "miss-sha-0123456789abcdef",
        &["billing", "preamble", "role-body"],
    );

    let detail = RequestDetail::from_request(&req, &empty_composer_cache(), None);
    let projection = detail
        .system_blocks
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("system_blocks must be Some when blocks are stored"))?;
    assert!(
        projection.composition_version.is_none(),
        "composition_version must be None on cache miss"
    );
    for block in &projection.blocks {
        assert!(
            block.composition_match.is_none(),
            "every block must have composition_match=null on cache miss; \
             got {:?} at index {}",
            block.composition_match,
            block.index
        );
    }
    Ok(())
}

#[test]
pub(crate) fn system_blocks_projection_all_blocks_matched() -> anyhow::Result<()> {
    let sha = "matched-sha-fedcba9876543210";
    let texts = ["billing", "preamble", "role-body"];
    let mut req = sample_request();
    req.system_prompt = observation_with_blocks(sha, &texts);
    req.turn_origin = "primary_first".to_string();

    let mut cache = empty_composer_cache();
    cache.insert(
        crate::compose_prompt::PromptClassKey::Main,
        fixture_composition(sha, &texts),
    );

    let detail = RequestDetail::from_request(&req, &cache, None);
    let projection = detail
        .system_blocks
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("system_blocks must be Some"))?;
    assert_eq!(
        projection.composition_version,
        Some(7),
        "composition_version must mirror the cached row's version"
    );
    for block in &projection.blocks {
        assert_eq!(
            block.composition_match.as_deref(),
            Some("matched"),
            "block at index {} must be `matched`; got {:?}",
            block.index,
            block.composition_match
        );
    }
    Ok(())
}

#[test]
pub(crate) fn system_blocks_projection_extra_when_request_has_more_blocks() -> anyhow::Result<()> {
    let sha = "extra-sha-1111222233334444";
    let request_texts = ["b0", "b1", "b2", "b3-extra"];
    let composition_texts = ["b0", "b1", "b2"];
    let mut req = sample_request();
    req.system_prompt = observation_with_blocks(sha, &request_texts);
    req.turn_origin = "primary_first".to_string();

    let mut cache = empty_composer_cache();
    cache.insert(
        crate::compose_prompt::PromptClassKey::Main,
        fixture_composition(sha, &composition_texts),
    );

    let detail = RequestDetail::from_request(&req, &cache, None);
    let projection = detail
        .system_blocks
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("system_blocks must be Some"))?;
    assert_eq!(projection.composition_version, Some(7));

    for (i, block) in projection.blocks.iter().take(3).enumerate() {
        assert_eq!(
            block.composition_match.as_deref(),
            Some("matched"),
            "block index {i} must be `matched`; got {:?}",
            block.composition_match
        );
    }
    assert_eq!(
        projection.blocks[3].composition_match.as_deref(),
        Some("extra"),
        "block at request index 3 must be `extra` because the composition has only 3 blocks"
    );
    Ok(())
}

#[test]
pub(crate) fn system_blocks_projection_unknown_when_text_differs() -> anyhow::Result<()> {
    let sha = "unknown-sha-aaaabbbbccccdddd";
    let request_texts = ["billing", "preamble", "DRIFTED-ROLE-BODY"];
    let composition_texts = ["billing", "preamble", "role-body"];
    let mut req = sample_request();
    req.system_prompt = observation_with_blocks(sha, &request_texts);
    req.turn_origin = "primary_first".to_string();

    let mut cache = empty_composer_cache();
    cache.insert(
        crate::compose_prompt::PromptClassKey::Main,
        fixture_composition(sha, &composition_texts),
    );

    let detail = RequestDetail::from_request(&req, &cache, None);
    let projection = detail
        .system_blocks
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("system_blocks must be Some"))?;
    assert_eq!(projection.composition_version, Some(7));
    assert_eq!(
        projection.blocks[0].composition_match.as_deref(),
        Some("matched"),
    );
    assert_eq!(
        projection.blocks[1].composition_match.as_deref(),
        Some("matched"),
    );
    assert_eq!(
        projection.blocks[2].composition_match.as_deref(),
        Some("unknown"),
        "drifted block must surface `unknown`; got {:?}",
        projection.blocks[2].composition_match
    );
    Ok(())
}

#[test]
pub(crate) fn body_payload_from_bytes_non_utf8_yields_null_text() -> anyhow::Result<()> {
    let raw = Bytes::from_static(&[0xC0, 0xFF, 0xFE]);
    let payload = BodyPayload::from_bytes(Some(&raw));
    let value = serde_json::to_value(&payload)?;

    assert_eq!(value.get("has_body"), Some(&Value::Bool(true)));
    assert_eq!(value.get("byte_size"), Some(&Value::from(3u64)));
    assert_eq!(value.get("text"), Some(&Value::Null));
    assert!(value.get("truncated").is_none());
    Ok(())
}

pub(crate) fn sample_session_view() -> SessionView {
    SessionView {
        id: "sess-abc".to_string(),
        first_seen: fixed_time(2026, 4, 11, 12, 0),
        last_seen: fixed_time(2026, 4, 11, 12, 5),
        request_count: 3,
        models: vec![
            "claude-sonnet-4-20250514".to_string(),
            "claude-opus-4-5-20250929".to_string(),
        ],
        last_status: "Streaming".to_string(),
        pinned: false,
        ttl_override: None,
        title: Some("Optimize request polling efficiency".to_string()),
    }
}

#[test]
pub(crate) fn status_is_terminal_classifies_wire_strings() {
    assert!(super::status_is_terminal("Complete"));
    assert!(super::status_is_terminal("Error: upstream reset"));
    assert!(!super::status_is_terminal("Pending"));
    assert!(!super::status_is_terminal("Streaming"));
    assert!(!super::status_is_terminal(""));
}

#[test]
pub(crate) fn compute_session_busy_is_registry_or_non_terminal() {
    assert!(super::compute_session_busy(true, "Complete"));
    assert!(super::compute_session_busy(false, "Streaming"));
    assert!(!super::compute_session_busy(false, "Complete"));
    assert!(!super::compute_session_busy(false, "Error: boom"));
}

#[test]
pub(crate) fn session_etag_changes_when_the_verdict_does_but_the_requests_do_not() {
    let (seen, count, busy) = ("2026-08-05T17:55:38+00:00", 905, false);

    let stale = super::session_etag(seen, count, busy, Some("2026-08-05T08:27:04+00:00"));
    let rescored = super::session_etag(seen, count, busy, Some("2026-08-07T20:23:47+00:00"));
    assert_ne!(
        stale, rescored,
        "a re-scored verdict must bust the validator even though last_seen, \
         request_count and busy are all unchanged"
    );

    let unjudged = super::session_etag(seen, count, busy, None);
    assert_ne!(
        unjudged, stale,
        "a session gaining its first verdict must bust the validator too"
    );

    assert_ne!(
        super::session_etag(seen, count + 1, busy, None),
        unjudged,
        "a new request must still bust the validator"
    );
    assert_ne!(
        super::session_etag(seen, count, true, None),
        unjudged,
        "the streaming→finished transition must still bust the validator"
    );
}

#[test]
pub(crate) fn if_none_match_contains_handles_wildcard_list_and_weak() {
    let etag = "W/\"2026-3-false\"";
    assert!(super::if_none_match_contains("W/\"2026-3-false\"", etag));
    assert!(super::if_none_match_contains("*", etag));
    assert!(super::if_none_match_contains(
        "W/\"other\", W/\"2026-3-false\"",
        etag
    ));
    assert!(super::if_none_match_contains("\"2026-3-false\"", etag));
    assert!(!super::if_none_match_contains("W/\"2026-4-false\"", etag));
}

#[test]
pub(crate) fn session_summary_serializes_with_expected_keys() -> anyhow::Result<()> {
    use std::collections::HashSet;

    let view = sample_session_view();
    let summary = SessionSummary::from(&view);
    let value = serde_json::to_value(&summary)?;

    let map = value
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("SessionSummary must serialize to a JSON object"))?;

    let actual: HashSet<String> = map.keys().cloned().collect();
    let expected: HashSet<String> = [
        "id",
        "first_seen",
        "last_seen",
        "request_count",
        "models",
        "last_status",
        "project",
        "project_id",
        "cwd",
        "project_resolution",
        "git_repo",
        "pinned",
        "ttl_override",
        "title",
        "avg_latency_ms",
        "originating_operator_prompt",
        "session_outcome",
        "session_quality_score",
        "session_judged_at",
        "narrative_overview",
        "narrative_overview_generated_at",
        "busy",
        "pending_inbox_count",
        "projects_touched",
    ]
    .into_iter()
    .map(str::to_string)
    .collect();

    assert_eq!(
        actual, expected,
        "SessionSummary JSON keys drift: got {actual:?}, want {expected:?}"
    );

    assert_eq!(
        map.get("busy"),
        Some(&Value::Bool(true)),
        "non-terminal last_status must seed busy=true"
    );

    assert_eq!(map.get("id"), Some(&Value::String("sess-abc".to_string())));
    assert_eq!(map.get("request_count"), Some(&Value::from(3u64)));
    let models = map
        .get("models")
        .and_then(Value::as_array)
        .ok_or_else(|| anyhow::anyhow!("models must be an array"))?;
    assert_eq!(models.len(), 2);
    assert_eq!(
        models[0],
        Value::String("claude-sonnet-4-20250514".to_string()),
        "first model must be the chronologically oldest distinct model"
    );
    assert_eq!(
        map.get("last_status"),
        Some(&Value::String("Streaming".to_string()))
    );
    assert_eq!(map.get("project"), Some(&Value::Null));
    assert_eq!(
        map.get("project_resolution"),
        Some(&Value::String("unresolved".to_string()))
    );
    assert_eq!(
        map.get("title"),
        Some(&Value::String(
            "Optimize request polling efficiency".to_string()
        )),
        "title must surface the sample view's parsed title verbatim"
    );

    Ok(())
}

pub(crate) fn sample_session_summary_row_matching_view() -> SessionSummaryRow {
    let view = sample_session_view();
    SessionSummaryRow {
        session_id: view.id.clone(),
        project_path: "(no project)".to_string(),
        project_id: None,
        cwd: None,
        project_resolution: "unresolved".to_string(),
        first_seen: view.first_seen,
        last_seen: view.last_seen,
        request_count: view.request_count as i32,
        last_status: Some(view.last_status.clone()),
        last_models: view.models.clone(),
        title: view.title.clone(),
        pinned: view.pinned,
        project_pinned: false,
        avg_latency_ms: None,
        originating_operator_prompt: None,
        session_outcome: None,
        session_quality_score: None,
        session_judged_at: None,
        overview: None,
        overview_generated_at: None,
        pending_inbox_count: 0,
        projects_touched: Vec::new(),
    }
}
