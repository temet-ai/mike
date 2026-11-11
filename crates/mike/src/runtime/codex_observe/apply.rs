use chrono::{DateTime, Utc};
use sqlx::PgPool;
use uuid::Uuid;

use super::hook::{HookEvent, HookPayload};
use super::otel::{RequestAttempt, TelemetryFact, ToolResult};
use super::tool_kind;
use super::writes::{self, RequestFact, StoreError, ThreadFact, ToolFact, TurnFact};

const HOOK: &str = "hook";
const TELEMETRY: &str = "telemetry";

pub(crate) struct Delivery<'a> {
    pub pool: &'a PgPool,
    pub namespace_id: Uuid,
    pub ingestion_id: Uuid,
    pub at: DateTime<Utc>,
}

fn hook_thread(payload: &HookPayload) -> ThreadFact {
    let scope = payload.scope();
    let event = payload.event();
    ThreadFact {
        thread_id: scope.thread_id,
        root_thread_id: scope.root_thread_id,
        parent_thread_id: scope.parent_thread_id,
        agent_type: payload.agent_type.clone(),
        cwd: payload.cwd.clone(),
        model: payload.model.clone(),
        ended_reason: match event {
            HookEvent::SessionEnd => {
                Some(payload.reason.clone().unwrap_or_else(|| "unknown".into()))
            }
            HookEvent::SubagentStop => Some("subagent_stop".into()),
            _ => None,
        },
        source: HOOK,
        ..ThreadFact::default()
    }
}

fn hook_turn(payload: &HookPayload, turn_id: &str) -> TurnFact {
    let event = payload.event();
    TurnFact {
        turn_id: turn_id.to_owned(),
        thread_id: payload.scope().thread_id,
        prompt_submitted: event == HookEvent::UserPromptSubmit,
        requests_started: i32::from(event == HookEvent::PreModelRequest),
        requests_finished: i32::from(event == HookEvent::PostModelRequest),
        stopped: matches!(event, HookEvent::Stop | HookEvent::SubagentStop),
        last_assistant_message: payload.last_assistant_message.clone(),
        source: HOOK,
    }
}

fn hook_tool(payload: &HookPayload, call_id: &str) -> ToolFact {
    let event = payload.event();
    let name = payload.tool_name.clone().unwrap_or_default();
    ToolFact {
        call_id: call_id.to_owned(),
        thread_id: Some(payload.scope().thread_id),
        turn_id: payload.turn_id.clone(),
        kind: tool_kind::from_hook_name(&name).as_str(),
        hook_tool_name: Some(name),
        input: payload.tool_input.clone(),
        started: event == HookEvent::PreToolUse,
        completed: event == HookEvent::PostToolUse,
        hook_response_seen: event == HookEvent::PostToolUse && payload.tool_response.is_some(),
        source: HOOK,
        ..ToolFact::default()
    }
}

fn hook_request(payload: &HookPayload) -> Option<RequestFact> {
    let phase = match payload.event() {
        HookEvent::PreModelRequest => "started",
        HookEvent::PostModelRequest => "finished",
        _ => return None,
    };
    let error = payload.error_text();
    Some(RequestFact {
        source: HOOK,
        phase,
        thread_id: Some(payload.scope().thread_id),
        turn_id: payload.turn_id.clone(),
        status: payload.status.clone(),
        success: payload
            .status
            .as_deref()
            .map(|status| status == "completed" && error.is_none()),
        error,
        usage: payload.token_usage.clone(),
        ..RequestFact::default()
    })
}

pub(crate) async fn hook(
    delivery: &Delivery<'_>,
    payload: &HookPayload,
) -> Result<usize, StoreError> {
    let (pool, ns, at) = (delivery.pool, delivery.namespace_id, delivery.at);
    let thread = hook_thread(payload);
    if let Some(root) = thread.parent_thread_id.clone() {
        let root_fact = ThreadFact {
            thread_id: root.clone(),
            root_thread_id: root,
            source: HOOK,
            ..ThreadFact::default()
        };
        writes::upsert_thread(pool, ns, &root_fact, at).await?;
    }
    writes::upsert_thread(pool, ns, &thread, at).await?;
    let mut facts = 1;
    if let Some(turn_id) = payload
        .turn_id
        .as_deref()
        .filter(|id| !id.trim().is_empty())
    {
        writes::upsert_turn(pool, ns, &hook_turn(payload, turn_id), at).await?;
        facts += 1;
    }
    let tool_event = matches!(
        payload.event(),
        HookEvent::PreToolUse | HookEvent::PostToolUse
    );
    if let Some(call_id) = payload
        .tool_use_id
        .as_deref()
        .filter(|id| tool_event && !id.trim().is_empty())
    {
        writes::upsert_tool(pool, ns, &hook_tool(payload, call_id), at).await?;
        facts += 1;
    }
    if let Some(request) = hook_request(payload) {
        writes::insert_request(pool, ns, delivery.ingestion_id, 0, &request, at).await?;
        facts += 1;
    }
    Ok(facts)
}

fn telemetry_tool(result: &ToolResult) -> ToolFact {
    let kind = tool_kind::from_telemetry(
        result.namespace.as_deref(),
        &result.tool_name,
        result.mcp_server.as_deref(),
    );
    let still_running = result.process_session.is_some() && result.exit_code.is_none();
    ToolFact {
        call_id: result.call_id.clone(),
        thread_id: result.thread_id.clone(),
        kind: kind.as_str(),
        telemetry_tool_name: Some(result.tool_name.clone()),
        telemetry_namespace: result.namespace.clone(),
        process_session: result.process_session.clone(),
        completed: !still_running,
        duration_ms: result.duration_ms,
        success: result.success,
        exit_code: result.exit_code,
        telemetry_output_snippet: true,
        source: TELEMETRY,
        ..ToolFact::default()
    }
}

fn telemetry_request(attempt: &RequestAttempt) -> RequestFact {
    let phase = if attempt.event == "codex.sse_event" {
        "response"
    } else {
        "attempt"
    };
    RequestFact {
        source: TELEMETRY,
        phase,
        thread_id: attempt.thread_id.clone(),
        endpoint: attempt.endpoint.clone(),
        attempt: attempt.attempt,
        status: Some(attempt.event.clone()),
        http_status: attempt.http_status,
        success: attempt.success,
        error: attempt.error.clone(),
        duration_ms: attempt.duration_ms,
        usage: attempt.usage.clone(),
        ..RequestFact::default()
    }
}

async fn telemetry_fact(
    delivery: &Delivery<'_>,
    ordinal: i32,
    fact: &TelemetryFact,
) -> Result<(), StoreError> {
    let (pool, ns) = (delivery.pool, delivery.namespace_id);
    match fact {
        TelemetryFact::Conversation {
            thread_id,
            model,
            codex_version,
            originator,
            at,
        } => {
            let thread = ThreadFact {
                thread_id: thread_id.clone(),
                root_thread_id: thread_id.clone(),
                model: model.clone(),
                codex_version: codex_version.clone(),
                originator: originator.clone(),
                source: TELEMETRY,
                ..ThreadFact::default()
            };
            writes::upsert_thread(pool, ns, &thread, *at).await?;
        }
        TelemetryFact::Spawn {
            parent_thread_id,
            child_thread_id,
            at,
        } => {
            let child = ThreadFact {
                thread_id: child_thread_id.clone(),
                root_thread_id: parent_thread_id.clone(),
                parent_thread_id: Some(parent_thread_id.clone()),
                source: TELEMETRY,
                ..ThreadFact::default()
            };
            writes::upsert_thread(pool, ns, &child, *at).await?;
        }
        TelemetryFact::ToolDecision {
            thread_id,
            call_id,
            tool_name,
            namespace,
            at,
        } => {
            let tool = ToolFact {
                call_id: call_id.clone(),
                thread_id: thread_id.clone(),
                kind: tool_kind::from_telemetry(namespace.as_deref(), tool_name, None).as_str(),
                telemetry_tool_name: Some(tool_name.clone()),
                telemetry_namespace: namespace.clone(),
                started: true,
                source: TELEMETRY,
                ..ToolFact::default()
            };
            writes::upsert_tool(pool, ns, &tool, *at).await?;
        }
        TelemetryFact::ToolResult(result) => {
            writes::upsert_tool(pool, ns, &telemetry_tool(result), result.at).await?;
        }
        TelemetryFact::Request(attempt) => {
            let request = telemetry_request(attempt);
            writes::insert_request(
                pool,
                ns,
                delivery.ingestion_id,
                ordinal,
                &request,
                attempt.at,
            )
            .await?;
        }
    }
    Ok(())
}

pub(crate) async fn telemetry(
    delivery: &Delivery<'_>,
    facts: &[TelemetryFact],
) -> Result<usize, StoreError> {
    for (ordinal, fact) in (0_i32..).zip(facts) {
        telemetry_fact(delivery, ordinal, fact).await?;
    }
    Ok(facts.len())
}
