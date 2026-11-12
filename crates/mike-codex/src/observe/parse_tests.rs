use serde_json::Value;

use super::hook::{HookEvent, HookPayload};
use super::otel::{self, TelemetryFact};
use super::tool_kind::{self, ToolKind};

pub(super) const SUBAGENT_HOOKS: &str = include_str!("fixtures/subagent_run_hooks.jsonl");
pub(super) const SUBAGENT_LOGS: &str = include_str!("fixtures/subagent_run_otel_logs.json");
pub(super) const SUBAGENT_SPANS: &str = include_str!("fixtures/subagent_run_otel_hook_spans.json");
pub(super) const POLL_HOOKS: &str = include_str!("fixtures/code_mode_poll_run_hooks.jsonl");
pub(super) const POLL_LOGS: &str = include_str!("fixtures/code_mode_poll_run_otel_logs.json");

fn hooks(lines: &str) -> anyhow::Result<Vec<HookPayload>> {
    lines
        .lines()
        .filter(|line| !line.trim().is_empty())
        .map(|line| HookPayload::parse(line.as_bytes()).map_err(anyhow::Error::msg))
        .collect()
}

fn log_facts(exports: &str) -> anyhow::Result<Vec<TelemetryFact>> {
    let bodies: Vec<Value> = serde_json::from_str(exports)?;
    Ok(bodies.iter().flat_map(otel::log_facts).collect())
}

#[test]
fn recorded_hook_events_are_all_known_names() -> anyhow::Result<()> {
    let all = [hooks(SUBAGENT_HOOKS)?, hooks(POLL_HOOKS)?].concat();
    assert!(all.len() >= 40, "only {} recorded hook payloads", all.len());
    let unknown: Vec<_> = all
        .iter()
        .filter(|p| p.event() == HookEvent::Unknown)
        .collect();
    assert!(unknown.is_empty(), "unknown hook events {unknown:?}");
    Ok(())
}

#[test]
fn subagent_hook_keeps_parent_session_and_names_child_thread() -> anyhow::Result<()> {
    let payloads = hooks(SUBAGENT_HOOKS)?;
    let start = payloads
        .iter()
        .find(|p| p.event() == HookEvent::SubagentStart)
        .ok_or_else(|| anyhow::anyhow!("no SubagentStart in the recording"))?;
    let scope = start.scope();
    assert_eq!(
        scope.parent_thread_id.as_deref(),
        Some(start.session_id.as_str())
    );
    assert_eq!(scope.root_thread_id, start.session_id);
    assert_ne!(scope.thread_id, start.session_id);
    let child_tool = payloads
        .iter()
        .find(|p| {
            p.event() == HookEvent::PreToolUse
                && p.agent_id.as_deref() == Some(scope.thread_id.as_str())
        })
        .ok_or_else(|| anyhow::anyhow!("the child's own tool call reported no agent_id"))?;
    assert_eq!(child_tool.session_id, start.session_id);
    Ok(())
}

#[test]
fn spawn_telemetry_names_the_same_child_as_the_hook() -> anyhow::Result<()> {
    let child = hooks(SUBAGENT_HOOKS)?
        .into_iter()
        .find_map(|p| p.agent_id)
        .ok_or_else(|| anyhow::anyhow!("no agent_id in hooks"))?;
    let spawned: Vec<_> = log_facts(SUBAGENT_LOGS)?
        .into_iter()
        .filter_map(|fact| match fact {
            TelemetryFact::Spawn {
                child_thread_id, ..
            } => Some(child_thread_id),
            _ => None,
        })
        .collect();
    assert_eq!(spawned, vec![child]);
    Ok(())
}

#[test]
fn collaboration_tools_classify_as_subagent_control() -> anyhow::Result<()> {
    let names: Vec<String> = hooks(SUBAGENT_HOOKS)?
        .into_iter()
        .filter_map(|p| p.tool_name)
        .collect();
    assert!(
        names.iter().any(|n| n == "collaborationspawn_agent"),
        "names {names:?}"
    );
    for name in names.iter().filter(|n| n.starts_with("collaboration")) {
        assert_eq!(
            tool_kind::from_hook_name(name),
            ToolKind::SubagentControl,
            "{name}"
        );
    }
    assert_eq!(tool_kind::from_hook_name("Bash"), ToolKind::Shell);
    assert_eq!(tool_kind::from_hook_name("web_search"), ToolKind::Hosted);
    Ok(())
}

fn tool_results(exports: &str) -> anyhow::Result<Vec<otel::ToolResult>> {
    Ok(log_facts(exports)?
        .into_iter()
        .filter_map(|fact| match fact {
            TelemetryFact::ToolResult(result) => Some(result),
            _ => None,
        })
        .collect())
}

#[test]
fn long_command_polls_link_to_their_command_by_process_session() -> anyhow::Result<()> {
    let results = tool_results(POLL_LOGS)?;
    let started = results
        .iter()
        .find(|r| r.tool_name == "exec_command" && r.exit_code.is_none())
        .ok_or_else(|| anyhow::anyhow!("no yielded exec_command"))?;
    let session = started
        .process_session
        .clone()
        .ok_or_else(|| anyhow::anyhow!("no process session"))?;
    let polls: Vec<_> = results
        .iter()
        .filter(|r| r.tool_name == "write_stdin")
        .collect();
    assert_eq!(polls.len(), 3);
    assert!(
        polls
            .iter()
            .all(|p| p.process_session.as_deref() == Some(session.as_str()))
    );
    assert_eq!(
        polls.iter().filter_map(|p| p.exit_code).collect::<Vec<_>>(),
        vec![0]
    );
    Ok(())
}

#[test]
fn code_mode_outer_and_nested_calls_are_kept_apart() -> anyhow::Result<()> {
    let results = tool_results(POLL_LOGS)?;
    let outer: Vec<_> = results.iter().filter(|r| r.tool_name == "exec").collect();
    let nested: Vec<_> = results
        .iter()
        .filter(|r| r.tool_name == "exec_command")
        .collect();
    assert_eq!((outer.len(), nested.len()), (5, 2));
    for call in &outer {
        assert_eq!(
            tool_kind::from_telemetry(call.namespace.as_deref(), &call.tool_name, None),
            ToolKind::CodeMode
        );
    }
    let nested_ids: Vec<_> = hooks(POLL_HOOKS)?
        .into_iter()
        .filter_map(|p| p.tool_use_id)
        .collect();
    assert!(nested.iter().all(|call| nested_ids.contains(&call.call_id)));
    Ok(())
}

#[test]
fn hook_spans_record_the_deadline_that_expired() -> anyhow::Result<()> {
    let bodies: Vec<Value> = serde_json::from_str(SUBAGENT_SPANS)?;
    let runs: Vec<_> = bodies.iter().flat_map(otel::hook_runs).collect();
    let timeouts: Vec<_> = runs.iter().filter(|r| r.outcome == "timeout").collect();
    assert_eq!(timeouts.len(), 5);
    assert!(
        timeouts
            .iter()
            .all(|r| r.event_name == "PreToolUse" && r.timeout_sec == Some(2))
    );
    assert!(timeouts.iter().all(|r| r.duration_ms >= 1_900));
    assert!(
        runs.iter()
            .any(|r| r.execution_mode.as_deref() == Some("async"))
    );
    Ok(())
}

#[test]
fn malformed_hook_body_names_the_position() {
    let error = HookPayload::parse(b"{\"session_id\": ")
        .err()
        .unwrap_or_default();
    assert!(error.contains("line 1"), "{error}");
    let empty = HookPayload::parse(br#"{"hook_event_name":"Stop","session_id":"  "}"#)
        .err()
        .unwrap_or_default();
    assert!(empty.contains("session_id"), "{empty}");
}

#[test]
fn completed_call_uses_provider_time_and_stable_source_key() -> anyhow::Result<()> {
    let attributes = serde_json::json!([
        {"key":"event.name","value":{"stringValue":"codex.sse_event"}},
        {"key":"event.kind","value":{"stringValue":"response.completed"}},
        {"key":"conversation.id","value":{"stringValue":"thread"}},
        {"key":"event.timestamp","value":{"stringValue":"2026-11-12T12:00:00.123Z"}},
        {"key":"model","value":{"stringValue":"reported-model"}},
        {"key":"input_token_count","value":{"intValue":"42"}},
        {"key":"output_token_count","value":{"intValue":"7"}}
    ]);
    let body = serde_json::json!({"resourceLogs":[{"scopeLogs":[{"logRecords":[{"attributes":attributes}]}]}]});
    let call =
        |body: &Value| log_call(body).ok_or_else(|| anyhow::anyhow!("missing terminal usage"));
    let first = call(&body)?;
    assert_eq!(first.usage["input_tokens"], 42);
    assert_eq!(first.model.as_deref(), Some("reported-model"));
    let mut exported_again = body.clone();
    exported_again["resourceLogs"][0]["scopeLogs"][0]["logRecords"] = serde_json::json!([
        {"attributes":[]}, {"attributes":attributes}
    ]);
    assert_eq!(call(&exported_again)?.source_key, first.source_key);
    let mut missing_time = body;
    missing_time["resourceLogs"][0]["scopeLogs"][0]["logRecords"][0]["attributes"]
        .as_array_mut()
        .ok_or_else(|| anyhow::anyhow!("attributes"))?
        .remove(3);
    assert!(log_call(&missing_time).is_none());
    Ok(())
}

fn log_call(body: &Value) -> Option<otel::ObservedModelCall> {
    otel::log_facts(body)
        .into_iter()
        .find_map(|fact| match fact {
            TelemetryFact::Request(request) => request.completed_call(),
            _ => None,
        })
}
