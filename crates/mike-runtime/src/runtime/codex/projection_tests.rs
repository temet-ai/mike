use serde_json::json;

use super::test_support::{first_item, item_event, notification, observation, scope};
use super::*;

#[test]
fn snapshots_common_categories_preserve_original_provider_payloads() -> anyhow::Result<()> {
    let cases = [
        (
            "userMessage",
            ItemCategory::UserMessage,
            json!({"content":[{"type":"text","text":"hi"},{"type":"image","url":"data:synthetic"}]}),
        ),
        (
            "agentMessage",
            ItemCategory::AssistantMessage,
            json!({"text":"hello","phase":"commentary"}),
        ),
        (
            "commandExecution",
            ItemCategory::Command,
            json!({"command":"check","cwd":"/work","status":"inProgress","commandActions":[]}),
        ),
        (
            "mcpToolCall",
            ItemCategory::McpTool,
            json!({"server":"repo","tool":"read","arguments":{},"status":"completed","result":null}),
        ),
        (
            "dynamicToolCall",
            ItemCategory::CustomTool,
            json!({"tool":"custom","arguments":{},"status":"completed","success":false,"contentItems":[]}),
        ),
        (
            "functionCallOutput",
            ItemCategory::ToolOutput,
            json!({"name":"custom","output":{"body":"result"}}),
        ),
        (
            "fileChange",
            ItemCategory::FileEdit,
            json!({"changes":[{"path":"a.rs","kind":{"type":"update","move_path":null},"diff":"whole diff"}],"status":"declined"}),
        ),
        ("plan", ItemCategory::Plan, json!({"text":"plan"})),
        (
            "reasoning",
            ItemCategory::Reasoning,
            json!({"summary":["public emitted summary"],"content":[]}),
        ),
        ("contextCompaction", ItemCategory::Compaction, json!({})),
        (
            "collabAgentToolCall",
            ItemCategory::Collaboration,
            json!({"tool":"spawnAgent","receiverThreadIds":["child"],"senderThreadId":"parent","agentsStates":{},"status":"completed"}),
        ),
    ];
    for (provider_type, category, mut payload) in cases {
        payload["id"] = json!("Item:1");
        payload["type"] = json!(provider_type);
        payload["futureField"] = json!({"preserve":"a\0b"});
        let event = item_event("item/started", payload.clone())?;
        let ItemUpdate::Snapshot(snapshot) = event.update else {
            anyhow::bail!("missing snapshot");
        };
        assert_eq!(snapshot.category, category);
        assert_eq!(snapshot.provider_type, provider_type);
        assert_eq!(snapshot.provider_payload, payload);
        assert_eq!(event.key.thread_id, "Thread:Case/保留 ");
    }
    Ok(())
}

#[test]
fn unknown_item_type_is_visible_with_explicit_unsupported_issue() -> anyhow::Result<()> {
    let event = notification(
        "item/started",
        json!({
            "threadId":"t","turnId":"u","startedAtMs":1,
            "item":{"id":"i","type":"futureShape","opaque":{"field":true}}
        }),
    )?;
    let batch = decode_observation(&event, IntakeContext::new(None));
    assert_eq!(batch.status, ProjectionStatus::Partial);
    assert_eq!(batch.issues[0].kind, IssueKind::UnknownItemType);
    let item = first_item(&event, None)?;
    let ItemUpdate::Snapshot(snapshot) = item.update else {
        anyhow::bail!("missing snapshot");
    };
    assert_eq!(snapshot.category, ItemCategory::Unknown);
    assert_eq!(snapshot.provider_payload["opaque"]["field"], true);
    Ok(())
}

#[test]
fn malformed_item_ids_never_prevent_raw_capture() -> anyhow::Result<()> {
    for id in [json!(null), json!(""), json!("bad\0id"), json!(7)] {
        let event = notification(
            "item/started",
            json!({"threadId":"t","turnId":"u","item":{"id":id,"type":"plan","text":"whole text"}}),
        )?;
        let raw = event.raw_payload.clone();
        let batch = decode_observation(&event, IntakeContext::new(None));
        assert_eq!(batch.status, ProjectionStatus::Malformed);
        assert!(batch.effects.is_empty());
        assert_eq!(batch.issues[0].kind, IssueKind::InvalidIdentifier);
        assert_eq!(event.raw_payload, raw);
    }
    Ok(())
}

#[test]
fn signals_tool_request_does_not_treat_call_id_as_item_id() -> anyhow::Result<()> {
    let event = observation(json!({"id":19,"method":"item/tool/call","params":{
        "threadId":"t","turnId":"u","callId":"call:19","namespace":null,"tool":"mike_report","arguments":{"answer":"complete"}
    }}))?;
    let batch = decode_observation(&event, IntakeContext::new(None));
    let Some(ProjectionEffect::Signal(signal)) = batch.effects.first() else {
        anyhow::bail!("missing signal");
    };
    assert_eq!(signal.kind, SignalKind::CustomToolRequest);
    assert_eq!(signal.call_id, Field::Value("call:19".into()));
    assert_eq!(signal.item_id, Field::Missing);
    assert_eq!(signal.provider_payload["arguments"]["answer"], "complete");
    Ok(())
}

#[test]
fn signals_plan_diff_patch_progress_and_terminal_input_are_distinct_snapshots() -> anyhow::Result<()>
{
    let cases = [
        (
            "turn/plan/updated",
            SignalKind::TurnPlan,
            json!({"plan":[{"step":"check","status":"inProgress"}],"explanation":null}),
        ),
        (
            "turn/diff/updated",
            SignalKind::TurnDiff,
            json!({"diff":"whole turn diff"}),
        ),
        (
            "item/fileChange/patchUpdated",
            SignalKind::FilePatch,
            json!({"changes":[]}),
        ),
        (
            "item/mcpToolCall/progress",
            SignalKind::McpProgress,
            json!({"message":"still working"}),
        ),
        (
            "item/commandExecution/terminalInteraction",
            SignalKind::TerminalInput,
            json!({"processId":"p","stdin":"y\n"}),
        ),
    ];
    for (method, expected, mut params) in cases {
        params["threadId"] = json!("t");
        params["turnId"] = json!("u");
        params["itemId"] = json!("i");
        let event = notification(method, params.clone())?;
        let batch = decode_observation(&event, IntakeContext::new(None));
        let Some(ProjectionEffect::Signal(signal)) = batch.effects.first() else {
            anyhow::bail!("missing signal");
        };
        assert_eq!(signal.kind, expected);
        assert_eq!(signal.provider_payload, params);
    }
    Ok(())
}

#[test]
fn lifecycle_configured_model_is_never_assigned_as_actual_model() -> anyhow::Result<()> {
    let event = notification(
        "thread/started",
        json!({"thread":{
            "id":"child","sessionId":"shared-session","parentThreadId":"parent","forkedFromId":"ancestor",
            "model":"configured","modelProvider":"openai","cliVersion":"creating-version",
            "createdAt":1720000000i64,"status":{"type":"active","activeFlags":["waitingOnApproval"]}
        }}),
    )?;
    let batch = decode_observation(&event, IntakeContext::new(None));
    let Some(ProjectionEffect::Lifecycle(lifecycle)) = batch.effects.first() else {
        anyhow::bail!("missing lifecycle");
    };
    let thread = lifecycle
        .thread
        .as_ref()
        .ok_or_else(|| anyhow::anyhow!("missing thread"))?;
    assert_eq!(thread.configured_model, Field::Value("configured".into()));
    assert_eq!(thread.actual_model, Field::Missing);
    assert_eq!(thread.session_id, Field::Value("shared-session".into()));
    assert_eq!(thread.parent_thread_id, Field::Value("parent".into()));
    assert_eq!(thread.forked_from_id, Field::Value("ancestor".into()));
    assert_eq!(
        thread.creating_cli_version,
        Field::Value("creating-version".into())
    );
    assert_eq!(lifecycle.evidence.json_pointer, "/params/thread");
    Ok(())
}

#[test]
fn lifecycle_turn_timestamps_stay_seconds_and_interruption_stays_provider_state()
-> anyhow::Result<()> {
    let event = notification(
        "turn/completed",
        json!({"threadId":"t","turn":{
            "id":"u","status":"interrupted","startedAt":1720000000i64,"completedAt":1720000005i64,
            "durationMs":5000,"items":[],"itemsView":"notLoaded","error":{"message":"interrupted"}
        }}),
    )?;
    let batch = decode_observation(&event, IntakeContext::new(None));
    let Some(ProjectionEffect::Lifecycle(lifecycle)) = batch.effects.first() else {
        anyhow::bail!("missing lifecycle");
    };
    assert_eq!(lifecycle.started_at_seconds, Field::Value(1720000000i64));
    assert_eq!(lifecycle.completed_at_seconds, Field::Value(1720000005i64));
    assert_eq!(lifecycle.status, Field::Value(json!("interrupted")));
    assert_eq!(lifecycle.provider_payload["itemsView"], "notLoaded");
    Ok(())
}

#[test]
fn history_bad_entry_does_not_discard_valid_siblings_or_page_metadata() -> anyhow::Result<()> {
    let event = observation(json!({"id":"Request:Mixed/Case","result":{
        "data":[{"turnId":"Turn:A","item":{"id":"valid","type":"plan","text":"full"}},
        {"turnId":"Turn:A","item":{"id":"bad\0id","type":"plan","text":"kept raw"}}],
        "nextCursor":"page:continue"
    }}))?;
    let batch = decode_observation(
        &event,
        IntakeContext::new(Some(&scope("thread/items/list"))),
    );
    assert_eq!(batch.status, ProjectionStatus::Partial);
    assert_eq!(batch.effects.len(), 1);
    assert_eq!(batch.issues.len(), 1);
    assert_eq!(
        event
            .parsed_payload
            .as_ref()
            .and_then(|value| value.pointer("/result/nextCursor")),
        Some(&json!("page:continue"))
    );
    Ok(())
}

#[test]
fn envelope_conflicting_association_rejects_effect_without_rebinding() -> anyhow::Result<()> {
    let mut event = notification(
        "item/agentMessage/delta",
        json!({"threadId":"actual","turnId":"u","itemId":"i","delta":"full"}),
    )?;
    event.envelope.provider_thread_id = Some("foreign".into());
    let batch = decode_observation(&event, IntakeContext::new(None));
    assert_eq!(batch.status, ProjectionStatus::Malformed);
    assert!(batch.effects.is_empty());
    assert_eq!(batch.issues[0].kind, IssueKind::ConflictingAssociation);
    Ok(())
}

#[test]
fn connection_scoped_process_output_is_not_projected_as_agent_command() -> anyhow::Result<()> {
    for method in ["command/exec/outputDelta", "process/outputDelta"] {
        let event = notification(method, json!({"processId":"p","deltaBase64":"aGk="}))?;
        let batch = decode_observation(&event, IntakeContext::ordinary());
        assert_eq!(batch.status, ProjectionStatus::Unsupported);
        assert!(batch.effects.is_empty());
    }
    Ok(())
}

#[test]
fn lifecycle_conflicting_session_tree_is_independent_of_matching_thread_id() -> anyhow::Result<()> {
    let mut event = notification(
        "thread/started",
        json!({"thread":{"id":"t","sessionId":"tree-B"}}),
    )?;
    event.envelope.provider_thread_id = Some("t".into());
    event.envelope.provider_session_id = Some("tree-A".into());
    let batch = decode_observation(&event, IntakeContext::default());
    assert_eq!(batch.status, ProjectionStatus::Malformed);
    assert!(batch.effects.is_empty());
    assert_eq!(batch.issues[0].kind, IssueKind::ConflictingAssociation);
    Ok(())
}

#[test]
fn lifecycle_optional_identity_fields_empty_nul_or_wrong_type_remain_invalid() -> anyhow::Result<()>
{
    for key in ["sessionId", "parentThreadId", "forkedFromId"] {
        for invalid in [json!(""), json!("bad\0id"), json!(3)] {
            let mut thread = json!({"id":"t"});
            thread[key] = invalid.clone();
            let event = notification("thread/started", json!({"thread":thread}))?;
            let batch = decode_observation(&event, IntakeContext::default());
            assert_eq!(batch.status, ProjectionStatus::Partial);
            assert_eq!(batch.issues[0].kind, IssueKind::InvalidIdentifier);
            assert_eq!(
                batch.issues[0].json_pointer,
                format!("/params/thread/{key}")
            );
            let Some(ProjectionEffect::Lifecycle(lifecycle)) = batch.effects.first() else {
                anyhow::bail!("missing lifecycle");
            };
            let thread = lifecycle
                .thread
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("missing thread"))?;
            let field = match key {
                "sessionId" => &thread.session_id,
                "parentThreadId" => &thread.parent_thread_id,
                _ => &thread.forked_from_id,
            };
            assert_eq!(field, &Field::Invalid(invalid));
        }
    }
    Ok(())
}

#[test]
fn invalid_item_id_diagnostics_point_to_live_and_history_source_fields() -> anyhow::Result<()> {
    let event = notification(
        "item/started",
        json!({"threadId":"t","turnId":"u","item":{"id":"bad\0id","type":"plan"}}),
    )?;
    assert_eq!(
        decode_observation(&event, IntakeContext::default()).issues[0].json_pointer,
        "/params/item/id"
    );
    let event = observation(
        json!({"id":"Request:Mixed/Case","result":{"data":[{"turnId":"Turn:A","item":{"id":"bad\0id","type":"plan"}}]}}),
    )?;
    assert_eq!(
        decode_observation(
            &event,
            IntakeContext::new(Some(&scope("thread/items/list")))
        )
        .issues[0]
            .json_pointer,
        "/result/data/0/item/id"
    );
    let event = notification(
        "item/agentMessage/delta",
        json!({"threadId":"","turnId":"u","itemId":"i","delta":"x"}),
    )?;
    assert_eq!(
        decode_observation(&event, IntakeContext::default()).issues[0].json_pointer,
        "/params/threadId"
    );
    Ok(())
}

#[test]
fn lifecycle_signal_and_usage_identifiers_point_to_original_live_fields() -> anyhow::Result<()> {
    let cases = [
        (
            "thread/started",
            json!({"thread":{"id":""}}),
            "/params/thread/id",
        ),
        ("thread/started", json!({}), "/params/thread"),
        (
            "turn/started",
            json!({"threadId":"t","turn":{"id":""}}),
            "/params/turn/id",
        ),
        ("turn/started", json!({"threadId":"t"}), "/params/turn"),
        (
            "turn/started",
            json!({"threadId":"","turn":{"id":"u"}}),
            "/params/threadId",
        ),
        ("thread/closed", json!({"threadId":""}), "/params/threadId"),
        (
            "turn/plan/updated",
            json!({"threadId":"t","turnId":""}),
            "/params/turnId",
        ),
        (
            "turn/diff/updated",
            json!({"threadId":"","turnId":"u"}),
            "/params/threadId",
        ),
        (
            "thread/tokenUsage/updated",
            json!({"threadId":"","turnId":"u","tokenUsage":{}}),
            "/params/threadId",
        ),
        (
            "thread/tokenUsage/updated",
            json!({"threadId":"t","turnId":"","tokenUsage":{}}),
            "/params/turnId",
        ),
    ];
    for (method, params, pointer) in cases {
        let event = notification(method, params)?;
        let batch = decode_observation(&event, IntakeContext::default());
        assert_eq!(batch.status, ProjectionStatus::Malformed);
        assert_eq!(batch.issues[0].json_pointer, pointer);
    }
    Ok(())
}

#[test]
fn response_identity_errors_point_to_response_or_adapter_request_scope() -> anyhow::Result<()> {
    let cases = [
        (
            "thread/read",
            json!({"thread":{"id":""}}),
            "/result/thread/id",
        ),
        ("thread/read", json!({}), "/result/thread"),
        (
            "account/usage/read",
            json!({"threadUsage":{"threadId":""}}),
            "/result/threadUsage/threadId",
        ),
    ];
    for (method, result, pointer) in cases {
        let event = observation(json!({"id":"Request:Mixed/Case","result":result}))?;
        let batch = decode_observation(&event, IntakeContext::new(Some(&scope(method))));
        assert_eq!(batch.status, ProjectionStatus::Malformed);
        assert_eq!(batch.issues[0].json_pointer, pointer);
    }
    for method in ["account/usage/read", "thread/items/list"] {
        let mut rpc = scope(method);
        rpc.thread_id = Some("bad\0id".into());
        let result = json!({"threadUsage":null,"data":[{"turnId":"Turn:A","item":{"id":"i","type":"plan"}}]});
        let event = observation(json!({"id":"Request:Mixed/Case","result":result}))?;
        let batch = decode_observation(&event, IntakeContext::new(Some(&rpc)));
        assert_eq!(batch.status, ProjectionStatus::Malformed);
        assert_eq!(batch.issues[0].json_pointer, "/request/threadId");
    }
    Ok(())
}
