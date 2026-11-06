use serde_json::Value;

use super::project::{LifecycleObservation, SignalKind, SignalObservation, ThreadFacts};
use super::protocol::{
    Evidence, Field, IssueKind, ProjectionIssue, field, identifier, optional_identifier,
};

pub(super) fn decode_lifecycle(
    method: &str,
    params: &Value,
    mut evidence: Evidence,
) -> Result<Option<LifecycleObservation>, ProjectionIssue> {
    let (thread_id, turn_id, payload, thread) = match method {
        "thread/started" => {
            let value = params.get("thread").ok_or_else(|| {
                ProjectionIssue::new(IssueKind::MissingField, "/thread")
                    .prefixed(&evidence.json_pointer)
            })?;
            evidence.json_pointer.push_str("/thread");
            (
                identifier(value, "/id").map_err(|issue| issue.prefixed(&evidence.json_pointer))?,
                None,
                value,
                Some(thread_facts(value)),
            )
        }
        "turn/started" | "turn/completed" => {
            let thread_id = identifier(params, "/threadId")
                .map_err(|issue| issue.prefixed(&evidence.json_pointer))?;
            let value = params.get("turn").ok_or_else(|| {
                ProjectionIssue::new(IssueKind::MissingField, "/turn")
                    .prefixed(&evidence.json_pointer)
            })?;
            evidence.json_pointer.push_str("/turn");
            (
                thread_id,
                Some(
                    identifier(value, "/id")
                        .map_err(|issue| issue.prefixed(&evidence.json_pointer))?,
                ),
                value,
                None,
            )
        }
        "thread/status/changed"
        | "thread/closed"
        | "thread/archived"
        | "thread/unarchived"
        | "thread/deleted"
        | "thread/reverted" => (
            identifier(params, "/threadId")
                .map_err(|issue| issue.prefixed(&evidence.json_pointer))?,
            None,
            params,
            None,
        ),
        _ => return Ok(None),
    };
    Ok(Some(LifecycleObservation {
        method: method.into(),
        thread_id,
        turn_id,
        evidence,
        status: field(payload, "status"),
        started_at_seconds: field(payload, "startedAt"),
        completed_at_seconds: field(payload, "completedAt"),
        thread,
        provider_payload: payload.clone(),
    }))
}

fn thread_facts(value: &Value) -> ThreadFacts {
    ThreadFacts {
        configured_model: field(value, "model"),
        actual_model: Field::Missing,
        model_provider: field(value, "modelProvider"),
        session_id: optional_identifier(value, "sessionId"),
        parent_thread_id: optional_identifier(value, "parentThreadId"),
        forked_from_id: optional_identifier(value, "forkedFromId"),
        creating_cli_version: field(value, "cliVersion"),
    }
}

pub(super) fn decode_signal(
    method: &str,
    params: &Value,
    evidence: Evidence,
) -> Result<Option<SignalObservation>, ProjectionIssue> {
    let Some(kind) = signal_kind(method) else {
        return Ok(None);
    };
    validate_signal(params, kind).map_err(|issue| issue.prefixed(&evidence.json_pointer))?;
    Ok(Some(SignalObservation {
        kind,
        thread_id: identifier(params, "/threadId")
            .map_err(|issue| issue.prefixed(&evidence.json_pointer))?,
        turn_id: field(params, "turnId"),
        item_id: field(params, "itemId"),
        call_id: field(params, "callId"),
        evidence,
        provider_payload: params.clone(),
    }))
}

fn signal_kind(method: &str) -> Option<SignalKind> {
    match method {
        "turn/plan/updated" => Some(SignalKind::TurnPlan),
        "turn/diff/updated" => Some(SignalKind::TurnDiff),
        "item/fileChange/patchUpdated" => Some(SignalKind::FilePatch),
        "item/commandExecution/terminalInteraction" => Some(SignalKind::TerminalInput),
        "item/mcpToolCall/progress" => Some(SignalKind::McpProgress),
        "item/tool/call" => Some(SignalKind::CustomToolRequest),
        "item/commandExecution/requestApproval"
        | "item/fileChange/requestApproval"
        | "item/permissions/requestApproval" => Some(SignalKind::ApprovalRequest),
        "item/tool/requestUserInput" => Some(SignalKind::UserInputRequest),
        "item/reasoning/summaryPartAdded" => Some(SignalKind::ReasoningPart),
        "model/rerouted" => Some(SignalKind::ModelRerouted),
        "thread/compacted" => Some(SignalKind::Compaction),
        "error" => Some(SignalKind::Error),
        _ => None,
    }
}

fn validate_signal(params: &Value, kind: SignalKind) -> Result<(), ProjectionIssue> {
    for key in ["turnId", "itemId", "callId"] {
        if params.get(key).is_some_and(|value| !value.is_null()) {
            identifier(params, &format!("/{key}"))?;
        }
    }
    match kind {
        SignalKind::CustomToolRequest => {
            identifier(params, "/turnId")?;
            identifier(params, "/callId")?;
            identifier(params, "/tool")?;
        }
        SignalKind::TerminalInput
        | SignalKind::FilePatch
        | SignalKind::McpProgress
        | SignalKind::ReasoningPart => {
            identifier(params, "/turnId")?;
            identifier(params, "/itemId")?;
        }
        SignalKind::TurnPlan | SignalKind::TurnDiff | SignalKind::ModelRerouted => {
            identifier(params, "/turnId")?;
        }
        _ => {}
    }
    Ok(())
}
