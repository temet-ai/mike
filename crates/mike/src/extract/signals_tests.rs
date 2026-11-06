use super::*;
use serde_json::json;

fn parts_for(
    uri: &str,
    session_id: Option<&str>,
    extra_headers: &[(String, String)],
) -> anyhow::Result<Parts> {
    let mut builder = axum::http::Request::builder().method("POST").uri(uri);
    if let Some(id) = session_id {
        builder = builder.header("x-claude-code-session-id", id);
    }
    for (name, value) in extra_headers {
        builder = builder.header(name, value);
    }
    let (parts, ()) = builder.body(())?.into_parts();
    Ok(parts)
}

async fn test_state() -> anyhow::Result<AppState> {
    let config = crate::config::ProxyConfig::for_tests_in_memory();
    AppState::new(config).await
}

#[tokio::test]
async fn from_parts_and_body_reports_the_miss_without_a_session_header() -> anyhow::Result<()> {
    let state = test_state().await?;
    let parts = parts_for("https://api.anthropic.com/v1/messages", None, &[])?;
    let body = Bytes::from_static(b"{}");
    let signals = RequestSignals::from_parts_and_body(&parts, &body, &state).await;
    assert!(
        matches!(signals, Err(request::SessionIdMiss::Missing)),
        "a request with no canonical session id must not be captured, \
         and must report why so the uncaptured counter can label it"
    );
    Ok(())
}

#[tokio::test]
async fn from_parts_and_body_derives_model_and_streaming() -> anyhow::Result<()> {
    let state = test_state().await?;
    let parts = parts_for(
        "https://api.anthropic.com/v1/messages",
        Some("11111111-1111-1111-1111-111111111111"),
        &[],
    )?;
    let body = Bytes::from_static(
        br#"{"model":"claude-sonnet-4","stream":true,"messages":[{"role":"user","content":[{"type":"text","text":"hi"}]}]}"#,
    );
    let signals = RequestSignals::from_parts_and_body(&parts, &body, &state)
        .await
        .map_err(|miss| {
            anyhow::anyhow!("expected signals for a request with a session id, got {miss:?}")
        })?;
    assert_eq!(signals.session_id, "11111111-1111-1111-1111-111111111111");
    assert_eq!(signals.model.as_deref(), Some("claude-sonnet-4"));
    assert!(signals.is_streaming);
    assert_eq!(signals.message_count, 1);
    Ok(())
}

#[tokio::test]
async fn from_parts_and_body_overrides_turn_origin_for_count_tokens_path() -> anyhow::Result<()> {
    let state = test_state().await?;
    let parts = parts_for(
        "https://api.anthropic.com/v1/messages/count_tokens",
        Some("22222222-2222-2222-2222-222222222222"),
        &[],
    )?;
    let body = Bytes::from_static(br#"{"messages":[]}"#);
    let signals = RequestSignals::from_parts_and_body(&parts, &body, &state)
        .await
        .map_err(|miss| {
            anyhow::anyhow!("expected signals for a request with a session id, got {miss:?}")
        })?;
    assert_eq!(signals.turn_origin, TurnOrigin::CountTokensSidecar);
    Ok(())
}

#[tokio::test]
async fn from_parts_and_body_classifies_quota_probe_body_as_quota_probe_origin()
-> anyhow::Result<()> {
    let state = test_state().await?;
    let parts = parts_for(
        "https://api.anthropic.com/v1/messages",
        Some("44444444-4444-4444-4444-444444444444"),
        &[],
    )?;
    let body = Bytes::from_static(
        br#"{"model":"claude-haiku-4-5","max_tokens":1,"messages":[{"role":"user","content":"quota"}]}"#,
    );
    let signals = RequestSignals::from_parts_and_body(&parts, &body, &state)
        .await
        .map_err(|miss| {
            anyhow::anyhow!("expected signals for a request with a session id, got {miss:?}")
        })?;
    assert_eq!(signals.turn_origin, TurnOrigin::QuotaProbe);
    Ok(())
}

#[tokio::test]
async fn from_parts_and_body_brief_wins_primary_first_after_a_quota_probe() -> anyhow::Result<()> {
    use crate::store::models::{
        InterceptedRequest, RequestId, RequestStatus, SystemPromptObservation,
    };

    let state = test_state().await?;
    let session_id = "55555555-5555-5555-5555-555555555555";

    let probe_parts = parts_for(
        "https://api.anthropic.com/v1/messages",
        Some(session_id),
        &[],
    )?;
    let probe_body = Bytes::from_static(
        br#"{"model":"claude-haiku-4-5","max_tokens":1,"messages":[{"role":"user","content":"quota"}]}"#,
    );
    let probe_signals = RequestSignals::from_parts_and_body(&probe_parts, &probe_body, &state)
        .await
        .map_err(|miss| anyhow::anyhow!("expected signals for the quota probe, got {miss:?}"))?;
    assert_eq!(
        probe_signals.turn_origin,
        TurnOrigin::QuotaProbe,
        "the probe must classify as its own variant, never Primary"
    );

    {
        let mut store = state.store.write().await;
        store.insert(InterceptedRequest {
            id: RequestId::next(),
            upstream_request_id: None,
            upstream_message_id: None,
            session_id: session_id.to_string(),
            method: "POST".to_string(),
            path: "/v1/messages".to_string(),
            request_headers: vec![],
            request_body: None,
            model: Some("claude-haiku-4-5".to_string()),
            is_streaming: false,
            message_count: 1,
            is_tool_loop: false,
            last_user_message: "quota".to_string(),
            tool_names: vec![],
            params: RequestParams {
                max_tokens: Some(1),
                temperature: None,
                thinking_enabled: false,
                thinking_budget: None,
                thinking_mode: None,
                effort: None,
            },
            project: None,
            git_repo: None,
            human_prompt: "quota".to_string(),
            turn_type: "user_prompt".to_string(),
            turn_origin: probe_signals.turn_origin.to_string(),
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
        });
    }

    let brief_parts = parts_for(
        "https://api.anthropic.com/v1/messages",
        Some(session_id),
        &[],
    )?;
    let brief_body = Bytes::from_static(
        br#"{"model":"claude-sonnet-5","max_tokens":1024,"messages":[{"role":"user","content":"real operator brief"}]}"#,
    );
    let brief_signals = RequestSignals::from_parts_and_body(&brief_parts, &brief_body, &state)
        .await
        .map_err(|miss| anyhow::anyhow!("expected signals for the brief, got {miss:?}"))?;

    assert_eq!(
        brief_signals.turn_origin,
        TurnOrigin::PrimaryFirst,
        "the real brief must win PrimaryFirst even though the quota probe landed first"
    );

    Ok(())
}

#[tokio::test]
async fn from_parts_and_body_caps_header_count() -> anyhow::Result<()> {
    let state = test_state().await?;
    let extra_headers: Vec<(String, String)> = (0..(MAX_HEADERS + 5))
        .map(|i| (format!("x-test-header-{i}"), "v".to_string()))
        .collect();
    let parts = parts_for(
        "https://api.anthropic.com/v1/messages",
        Some("33333333-3333-3333-3333-333333333333"),
        &extra_headers,
    )?;
    let body = Bytes::from_static(b"{}");
    let signals = RequestSignals::from_parts_and_body(&parts, &body, &state)
        .await
        .map_err(|miss| {
            anyhow::anyhow!("expected signals for a request with a session id, got {miss:?}")
        })?;
    assert!(signals.request_headers.len() <= MAX_HEADERS);
    assert!(signals.dropped_header_count > 0);
    Ok(())
}

#[tokio::test]
async fn from_parts_and_body_agent_cli_session_persona_continuation_stays_primary()
-> anyhow::Result<()> {
    let state = test_state().await?;
    let session_id = "66666666-6666-6666-6666-666666666666";
    let sp = "You are Claude Code, Anthropic's official CLI for Claude.\nYou are a Rust Development Expert -- a senior engineer who writes production-grade, idiomatic, and type-safe Rust code.";

    let first_parts = parts_for(
        "https://api.anthropic.com/v1/messages",
        Some(session_id),
        &[],
    )?;
    let first_body_json = json!({
        "model": "claude-opus-4-8",
        "system": sp,
        "messages": [{"role": "user", "content": "design the migration"}]
    });
    let first = RequestSignals::from_parts_and_body(
        &first_parts,
        &Bytes::from(serde_json::to_vec(&first_body_json)?),
        &state,
    )
    .await
    .map_err(|miss| anyhow::anyhow!("expected signals for the first request, got {miss:?}"))?;
    assert_eq!(
        first.turn_origin,
        TurnOrigin::PrimaryFirst,
        "session's own first substantive request establishes its identity, persona or not"
    );

    {
        let mut store = state.store.write().await;
        store.seed_composable_session(session_id);
    }

    let continuation_parts = parts_for(
        "https://api.anthropic.com/v1/messages",
        Some(session_id),
        &[],
    )?;
    let continuation_body_json = json!({
        "model": "claude-opus-4-8",
        "system": sp,
        "messages": [
            {"role": "user", "content": "..."},
            {
                "role": "assistant",
                "content": [{"type": "tool_use", "id": "t1", "name": "Read", "input": {}}]
            },
            {
                "role": "user",
                "content": [{"type": "tool_result", "tool_use_id": "t1", "content": "..."}]
            }
        ]
    });
    let continuation = RequestSignals::from_parts_and_body(
        &continuation_parts,
        &Bytes::from(serde_json::to_vec(&continuation_body_json)?),
        &state,
    )
    .await
    .map_err(|miss| {
        anyhow::anyhow!("expected signals for the continuation request, got {miss:?}")
    })?;
    assert_eq!(
        continuation.turn_origin,
        TurnOrigin::Primary,
        "same persona as the session's own established identity — a continuation, not a nested subagent"
    );

    Ok(())
}

#[tokio::test]
async fn from_parts_and_body_nested_subagent_persona_still_classifies_subagent()
-> anyhow::Result<()> {
    let state = test_state().await?;
    let session_id = "77777777-7777-7777-7777-777777777777";
    let plain_sp = "You are Claude Code, Anthropic's official CLI for Claude.\nYou are an interactive agent that helps users according to your \"Output Style\" below.";

    let first_parts = parts_for(
        "https://api.anthropic.com/v1/messages",
        Some(session_id),
        &[],
    )?;
    let first_body_json = json!({
        "model": "claude-sonnet-5",
        "system": plain_sp,
        "messages": [{"role": "user", "content": "fix the bug"}]
    });
    let first = RequestSignals::from_parts_and_body(
        &first_parts,
        &Bytes::from(serde_json::to_vec(&first_body_json)?),
        &state,
    )
    .await
    .map_err(|miss| anyhow::anyhow!("expected signals for the first request, got {miss:?}"))?;
    assert_eq!(first.turn_origin, TurnOrigin::PrimaryFirst);

    {
        let mut store = state.store.write().await;
        store.seed_composable_session(session_id);
    }

    let dispatch_parts = parts_for(
        "https://api.anthropic.com/v1/messages",
        Some(session_id),
        &[],
    )?;
    let dispatch_sp = "You are Claude Code, Anthropic's official CLI for Claude.\nYou are a Verification Specialist — you prove things actually work.";
    let dispatch_body_json = json!({
        "model": "claude-opus-4-8",
        "system": dispatch_sp,
        "messages": [{"role": "user", "content": "verify this fix actually works"}]
    });
    let dispatch = RequestSignals::from_parts_and_body(
        &dispatch_parts,
        &Bytes::from(serde_json::to_vec(&dispatch_body_json)?),
        &state,
    )
    .await
    .map_err(|miss| anyhow::anyhow!("expected signals for the dispatch request, got {miss:?}"))?;
    assert_eq!(
        dispatch.turn_origin,
        TurnOrigin::Subagent,
        "different persona than this session's own established (plain) identity — a genuine nested dispatch"
    );

    Ok(())
}

#[tokio::test]
async fn from_parts_and_body_mutation_exempt_true_for_prompt_assistant_header() -> anyhow::Result<()>
{
    let state = test_state().await?;
    let parts = parts_for(
        "https://api.anthropic.com/v1/messages",
        Some("11111111-1111-1111-1111-111111111111"),
        &[("x-mike-client".to_string(), "prompt-assistant".to_string())],
    )?;
    let body = Bytes::from_static(b"{}");
    let signals = RequestSignals::from_parts_and_body(&parts, &body, &state)
        .await
        .map_err(|miss| anyhow::anyhow!("expected signals, got {miss:?}"))?;
    assert!(
        signals.mutation_exempt,
        "the prompt-assistant client header must mark the request mutation-exempt"
    );
    Ok(())
}

#[tokio::test]
async fn from_parts_and_body_mutation_exempt_false_without_header() -> anyhow::Result<()> {
    let state = test_state().await?;
    let parts = parts_for(
        "https://api.anthropic.com/v1/messages",
        Some("11111111-1111-1111-1111-111111111111"),
        &[],
    )?;
    let body = Bytes::from_static(b"{}");
    let signals = RequestSignals::from_parts_and_body(&parts, &body, &state)
        .await
        .map_err(|miss| anyhow::anyhow!("expected signals, got {miss:?}"))?;
    assert!(
        !signals.mutation_exempt,
        "ordinary operator traffic carries no x-mike-client header and must not be exempt"
    );
    Ok(())
}

fn workflow_fixture() -> anyhow::Result<Vec<Value>> {
    Ok(serde_json::from_str(include_str!(
        "../../tests/fixtures/dossier/workflow_agent_requests.json"
    ))?)
}

async fn classify_fixture_request(
    state: &AppState,
    session_id: &str,
    entry: &Value,
) -> anyhow::Result<RequestSignals> {
    let mut headers = Vec::new();
    if let Some(agent_id) = entry["agent_id"].as_str() {
        headers.push(("x-claude-code-agent-id".to_string(), agent_id.to_string()));
    }
    let parts = parts_for(
        "https://api.anthropic.com/v1/messages",
        Some(session_id),
        &headers,
    )?;
    let body = json!({
        "model": "claude-sonnet-5",
        "system": entry["system"],
        "messages": [{"role": "user", "content": entry["first_user_blocks"]}]
    });
    RequestSignals::from_parts_and_body(&parts, &Bytes::from(serde_json::to_vec(&body)?), state)
        .await
        .map_err(|miss| anyhow::anyhow!("expected signals for the fixture request, got {miss:?}"))
}

#[tokio::test]
async fn issue_354_requests_444177_444612_445292_classify_workflow_subagent() -> anyhow::Result<()>
{
    let state = test_state().await?;
    for entry in workflow_fixture()? {
        if entry["agent_id"].is_null() {
            continue;
        }
        let signals =
            classify_fixture_request(&state, "77777777-7777-7777-7777-777777777777", &entry)
                .await?;
        assert_eq!(
            signals.turn_origin,
            TurnOrigin::WorkflowSubagent,
            "request {} carries an agent id and the workflow markers",
            entry["request_id"]
        );
    }
    Ok(())
}

#[tokio::test]
async fn no_agent_id_request_classifies_primary() -> anyhow::Result<()> {
    let state = test_state().await?;
    for entry in workflow_fixture()? {
        if !entry["agent_id"].is_null() {
            continue;
        }
        let signals =
            classify_fixture_request(&state, "88888888-8888-8888-8888-888888888888", &entry)
                .await?;
        assert!(
            matches!(
                signals.turn_origin,
                TurnOrigin::Primary | TurnOrigin::PrimaryFirst
            ),
            "main-thread request {} must stay on the primary lane, got {:?}",
            entry["request_id"],
            signals.turn_origin
        );
    }
    Ok(())
}

#[tokio::test]
async fn agent_id_request_seen_first_never_becomes_primary_or_the_session_persona()
-> anyhow::Result<()> {
    let state = test_state().await?;
    let session_id = "99999999-9999-9999-9999-999999999999";
    let entries = workflow_fixture()?;
    let agent_entry = entries
        .iter()
        .find(|e| !e["agent_id"].is_null())
        .ok_or_else(|| anyhow::anyhow!("fixture has no agent-id request"))?;
    let main_entry = entries
        .iter()
        .find(|e| e["agent_id"].is_null())
        .ok_or_else(|| anyhow::anyhow!("fixture has no main-thread request"))?;

    let agent_first = classify_fixture_request(&state, session_id, agent_entry).await?;
    assert_eq!(agent_first.turn_origin, TurnOrigin::WorkflowSubagent);
    let main = classify_fixture_request(&state, session_id, main_entry).await?;
    assert_eq!(main.turn_origin, TurnOrigin::PrimaryFirst);
    let agent_again = classify_fixture_request(&state, session_id, agent_entry).await?;
    assert_eq!(agent_again.turn_origin, TurnOrigin::WorkflowSubagent);
    Ok(())
}
