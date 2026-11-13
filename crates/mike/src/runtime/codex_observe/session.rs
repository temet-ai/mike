use crate::store::persistence::runtime_repo::RuntimePersist as _;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::ingest::{IngestError, ObserveSource};
use crate::runtime::{
    ModelCallObservation, ObservationSource, RuntimeItemObservation, SessionControl,
    SessionLifecycle, SessionObservation, UsageSnapshot,
};
use crate::store::persistence::PersistHandle;
use mike_codex::observe::TelemetryFact;
use mike_codex::observe::{HookEvent, HookPayload};
use mike_codex::observe::{from_hook_name, from_telemetry};

pub(super) async fn hook(
    persist: &PersistHandle,
    source: &ObserveSource,
    session: Uuid,
    delivery: Uuid,
    payload: &HookPayload,
) -> Result<SessionObservation, IngestError> {
    let at = persist
        .runtime()
        .session_observation_time(session, delivery)
        .await?
        .unwrap_or_else(Utc::now);
    let mut observation = base(session, delivery, source, "hook", at);
    observation.lifecycle = match payload.event() {
        HookEvent::SessionStart | HookEvent::Stop | HookEvent::Interrupt => {
            Some(SessionLifecycle::Idle)
        }
        HookEvent::SessionEnd | HookEvent::SubagentStop => Some(SessionLifecycle::Ended),
        HookEvent::UserPromptSubmit | HookEvent::PreModelRequest | HookEvent::SubagentStart => {
            Some(SessionLifecycle::Active)
        }
        _ => None,
    };
    observation.requested_model = payload.model.clone();
    observation.item = hook_item(payload);
    observation.control = (source.hook_boundary && payload.agent_id.is_none())
        .then_some(SessionControl::HookBoundary);
    Ok(observation)
}

pub(super) fn telemetry(
    source: &ObserveSource,
    session: Uuid,
    delivery: Uuid,
    fact: &TelemetryFact,
) -> SessionObservation {
    let at = match fact {
        TelemetryFact::Conversation { at, .. }
        | TelemetryFact::Spawn { at, .. }
        | TelemetryFact::ToolDecision { at, .. } => *at,
        TelemetryFact::Request(request) => request.at,
        TelemetryFact::ToolResult(result) => result.at,
    };
    let mut observation = base(session, delivery, source, "otel", at);
    if let TelemetryFact::Conversation { model, .. } = fact {
        observation.requested_model = model.clone();
    }
    observation.item = telemetry_item(fact);
    if let TelemetryFact::Request(request) = fact
        && let Some(call) = request.completed_call()
    {
        observation.observation_id = Uuid::new_v5(&session, call.source_key.as_bytes());
        observation.observed_model = call.model;
        observation.source.completeness = "observed_calls_only".into();
        let token = |name| {
            call.usage
                .get(name)
                .and_then(serde_json::Value::as_i64)
                .filter(|n| *n >= 0)
        };
        observation.model_call = Some(ModelCallObservation {
            call_id: format!("otel:{}", observation.observation_id),
            status: Some("completed".into()),
            duration_ms: request.duration_ms,
            http_status: None,
            usage: Some(UsageSnapshot {
                scope: "own".into(),
                basis: "call".into(),
                input_tokens: token("input_tokens"),
                cached_input_tokens: token("cached_input_tokens"),
                output_tokens: token("output_tokens"),
                reasoning_output_tokens: token("reasoning_output_tokens"),
                total_tokens: None,
                context_window: None,
            }),
        });
    }
    observation
}

pub(super) fn thread(fact: &TelemetryFact) -> Option<&str> {
    match fact {
        TelemetryFact::Conversation { thread_id, .. } => Some(thread_id),
        TelemetryFact::Spawn {
            child_thread_id, ..
        } => Some(child_thread_id),
        TelemetryFact::ToolDecision { thread_id, .. } => thread_id.as_deref(),
        TelemetryFact::Request(request) => request.thread_id.as_deref(),
        TelemetryFact::ToolResult(result) => result.thread_id.as_deref(),
    }
}

fn base(
    session: Uuid,
    delivery: Uuid,
    source: &ObserveSource,
    kind: &str,
    at: DateTime<Utc>,
) -> SessionObservation {
    SessionObservation {
        session_id: session,
        observation_id: delivery,
        runtime: "codex".into(),
        source: ObservationSource {
            kind: kind.into(),
            version: source.codex_version.clone(),
            fidelity: "normalized_metadata".into(),
            completeness: "event_fields_only".into(),
        },
        observed_at: at,
        sequence: 0,
        lifecycle: None,
        requested_model: None,
        observed_model: None,
        usage: None,
        model_call: None,
        item: None,
        control: None,
    }
}

fn hook_item(payload: &HookPayload) -> Option<RuntimeItemObservation> {
    let (id, kind, status) = match payload.event() {
        HookEvent::PreToolUse => (payload.tool_use_id.clone()?, "tool", "in_progress"),
        HookEvent::PostToolUse => (payload.tool_use_id.clone()?, "tool", "completed"),
        HookEvent::UserPromptSubmit => (
            format!("turn:{}", payload.turn_id.as_ref()?),
            "turn",
            "in_progress",
        ),
        HookEvent::Stop => (
            format!("turn:{}", payload.turn_id.as_ref()?),
            "turn",
            "completed",
        ),
        _ => return None,
    };
    let kind = if kind == "tool" {
        from_hook_name(payload.tool_name.as_deref().unwrap_or_default()).as_str()
    } else {
        kind
    };
    Some(RuntimeItemObservation {
        item_id: id,
        turn_id: payload.turn_id.clone(),
        kind: kind.into(),
        name: payload.tool_name.clone(),
        status: Some(status.into()),
        success: None,
        exit_code: None,
    })
}

fn telemetry_item(fact: &TelemetryFact) -> Option<RuntimeItemObservation> {
    let mut item = RuntimeItemObservation {
        item_id: String::new(),
        turn_id: None,
        kind: "tool".into(),
        name: None,
        status: None,
        success: None,
        exit_code: None,
    };
    match fact {
        TelemetryFact::ToolDecision {
            call_id,
            tool_name,
            namespace,
            ..
        } => {
            item.kind = from_telemetry(namespace.as_deref(), tool_name, None)
                .as_str()
                .into();
            item.item_id = call_id.clone();
            item.name = Some(tool_name.clone());
            item.status = Some("in_progress".into());
        }
        TelemetryFact::ToolResult(result) => {
            item.kind = from_telemetry(
                result.namespace.as_deref(),
                &result.tool_name,
                result.mcp_server.as_deref(),
            )
            .as_str()
            .into();
            item.item_id = result.call_id.clone();
            item.name = Some(result.tool_name.clone());
            item.status = Some(
                if result.process_session.is_some() && result.exit_code.is_none() {
                    "in_progress"
                } else {
                    "completed"
                }
                .into(),
            );
            item.success = result.success;
            item.exit_code = result.exit_code;
        }
        _ => return None,
    }
    Some(item)
}
