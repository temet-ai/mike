use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::admission::{ExclusionReason, IntakeContext, exclusion};
use super::decode_items::{delta, delta_channel, history_snapshot, live_snapshot};
use super::decode_lifecycle::{decode_lifecycle, decode_signal};
use super::items::{ItemCategory, ItemObservation, ItemUpdate, SnapshotKind};
use super::protocol::{
    Evidence, EvidenceOrigin, Field, IssueKind, ProjectionIssue, ReceiptId, RpcScope,
    audit_routing, response_scope_matches,
};
use super::usage::{BillingEstimate, UsageObservation, decode_billing, decode_usage};
use crate::runtime::RuntimeObservation;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectionStatus {
    Projected,
    Partial,
    Unsupported,
    Malformed,
    Excluded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectionBatch {
    pub receipt: ReceiptId,
    pub status: ProjectionStatus,
    pub effects: Vec<ProjectionEffect>,
    pub issues: Vec<ProjectionIssue>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ThreadFacts {
    pub configured_model: Field<String>,
    pub actual_model: Field<String>,
    pub model_provider: Field<String>,
    pub session_id: Field<String>,
    pub parent_thread_id: Field<String>,
    pub forked_from_id: Field<String>,
    pub creating_cli_version: Field<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleObservation {
    pub method: String,
    pub thread_id: String,
    pub turn_id: Option<String>,
    pub evidence: Evidence,
    pub status: Field<Value>,
    pub started_at_seconds: Field<i64>,
    pub completed_at_seconds: Field<i64>,
    pub thread: Option<ThreadFacts>,
    pub provider_payload: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignalKind {
    TurnPlan,
    TurnDiff,
    FilePatch,
    TerminalInput,
    McpProgress,
    CustomToolRequest,
    ApprovalRequest,
    UserInputRequest,
    ReasoningPart,
    ModelRerouted,
    Compaction,
    Error,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignalObservation {
    pub kind: SignalKind,
    pub thread_id: String,
    pub turn_id: Field<String>,
    pub item_id: Field<String>,
    pub call_id: Field<String>,
    pub evidence: Evidence,
    pub provider_payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ProjectionEffect {
    Item(Box<ItemObservation>),
    Lifecycle(Box<LifecycleObservation>),
    Signal(Box<SignalObservation>),
    Usage(Box<UsageObservation>),
    Billing(Box<BillingEstimate>),
}

pub fn decode_observation(
    observation: &RuntimeObservation,
    context: IntakeContext<'_>,
) -> ProjectionBatch {
    let receipt = ReceiptId {
        namespace_id: observation.envelope.namespace_id,
        ingestion_id: observation.envelope.ingestion_id,
    };
    let mut batch = ProjectionBatch {
        receipt,
        status: ProjectionStatus::Projected,
        effects: Vec::new(),
        issues: Vec::new(),
    };
    let parsed = serde_json::from_slice::<Value>(&observation.raw_payload).ok();
    if let Some(reason) = exclusion(
        parsed.as_ref(),
        context,
        audit_routing(&observation.raw_payload),
    ) {
        batch.status = if reason == ExclusionReason::AmbiguousEnvelope {
            ProjectionStatus::Malformed
        } else {
            ProjectionStatus::Excluded
        };
        let kind = match reason {
            ExclusionReason::Authentication => IssueKind::ExcludedAuthentication,
            ExclusionReason::UncorrelatedResponse => IssueKind::UncorrelatedResponse,
            ExclusionReason::UnreviewedResponse => IssueKind::UnreviewedResponse,
            ExclusionReason::UnclassifiedTraffic => IssueKind::UnclassifiedTraffic,
            ExclusionReason::AmbiguousEnvelope => IssueKind::MalformedEnvelope,
        };
        batch.issues.push(ProjectionIssue::new(kind, ""));
        return batch;
    }
    let result = checked_projection(observation, parsed.as_ref(), context.rpc_scope, &mut batch);
    if let Err(issue) = result {
        batch.issues.push(issue);
    }
    classify_batch(&mut batch);
    batch
}

fn checked_projection(
    observation: &RuntimeObservation,
    parsed: Option<&Value>,
    scope: Option<&RpcScope>,
    batch: &mut ProjectionBatch,
) -> Result<(), ProjectionIssue> {
    observation
        .validate()
        .map_err(|_| ProjectionIssue::new(IssueKind::InvalidObservation, ""))?;
    let value = parsed.ok_or_else(|| ProjectionIssue::new(IssueKind::MalformedJson, ""))?;
    if !value.is_object() {
        return Err(ProjectionIssue::new(IssueKind::MalformedEnvelope, ""));
    }
    if let Some(method) = value.get("method") {
        let method = method
            .as_str()
            .ok_or_else(|| ProjectionIssue::new(IssueKind::MalformedEnvelope, "/method"))?;
        validate_request_id(value)?;
        let params = value
            .get("params")
            .filter(|params| params.is_object())
            .ok_or_else(|| ProjectionIssue::new(IssueKind::MalformedEnvelope, "/params"))?;
        project_live(method, params, batch)?;
    } else {
        project_response(value, scope, batch)?;
    }
    check_envelope_associations(observation, batch);
    Ok(())
}

fn validate_request_id(value: &Value) -> Result<(), ProjectionIssue> {
    if let Some(id) = value.get("id") {
        serde_json::from_value::<super::protocol::RequestId>(id.clone())
            .map_err(|_| ProjectionIssue::new(IssueKind::MalformedEnvelope, "/id"))?;
    }
    Ok(())
}

fn project_live(
    method: &str,
    params: &Value,
    batch: &mut ProjectionBatch,
) -> Result<(), ProjectionIssue> {
    let evidence = evidence(batch, "/params", EvidenceOrigin::LiveFrame);
    let effect = match method {
        "item/started" | "item/completed" => {
            let kind = if method == "item/started" {
                SnapshotKind::Started
            } else {
                SnapshotKind::Completed
            };
            let mut source = evidence;
            source.json_pointer.push_str("/item");
            ProjectionEffect::Item(Box::new(live_snapshot(params, kind, source)?))
        }
        "thread/tokenUsage/updated" => {
            ProjectionEffect::Usage(Box::new(decode_usage(params, evidence)?))
        }
        _ => project_other_live(method, params, evidence)?,
    };
    push_effect(batch, effect);
    Ok(())
}

fn project_other_live(
    method: &str,
    params: &Value,
    mut evidence: Evidence,
) -> Result<ProjectionEffect, ProjectionIssue> {
    if let Some(channel) = delta_channel(method, params)? {
        evidence.json_pointer.push_str("/delta");
        return delta(params, channel, evidence).map(|item| ProjectionEffect::Item(Box::new(item)));
    }
    if let Some(lifecycle) = decode_lifecycle(method, params, evidence.clone())? {
        return Ok(ProjectionEffect::Lifecycle(Box::new(lifecycle)));
    }
    if let Some(signal) = decode_signal(method, params, evidence)? {
        return Ok(ProjectionEffect::Signal(Box::new(signal)));
    }
    Err(ProjectionIssue::new(IssueKind::UnknownMethod, "/method"))
}

fn project_response(
    value: &Value,
    scope: Option<&RpcScope>,
    batch: &mut ProjectionBatch,
) -> Result<(), ProjectionIssue> {
    let scope = scope
        .filter(|scope| response_scope_matches(value, scope))
        .ok_or_else(|| ProjectionIssue::new(IssueKind::UncorrelatedResponse, "/id"))?;
    if value.get("error").is_some() {
        return Err(ProjectionIssue::new(IssueKind::RpcError, "/error"));
    }
    let result = value
        .get("result")
        .filter(|result| result.is_object())
        .ok_or_else(|| ProjectionIssue::new(IssueKind::MalformedEnvelope, "/result"))?;
    let source = evidence(batch, "/result", EvidenceOrigin::RpcResponse);
    match scope.method.as_str() {
        "thread/items/list" => project_item_page(result, scope, batch),
        "account/usage/read" => {
            let estimate = decode_billing(result, scope, source)?;
            push_effect(batch, ProjectionEffect::Billing(Box::new(estimate)));
            Ok(())
        }
        "thread/read" => {
            let lifecycle =
                decode_lifecycle("thread/started", result, source)?.ok_or_else(|| {
                    ProjectionIssue::new(IssueKind::MalformedEnvelope, "/result/thread")
                })?;
            if scope
                .thread_id
                .as_ref()
                .is_some_and(|id| *id != lifecycle.thread_id)
            {
                return Err(ProjectionIssue::new(
                    IssueKind::ConflictingAssociation,
                    "/result/thread/id",
                ));
            }
            push_effect(
                batch,
                ProjectionEffect::Lifecycle(Box::new(LifecycleObservation {
                    method: scope.method.clone(),
                    ..lifecycle
                })),
            );
            Ok(())
        }
        _ => Err(ProjectionIssue::new(IssueKind::UnknownMethod, "/result")),
    }
}

fn project_item_page(
    result: &Value,
    scope: &RpcScope,
    batch: &mut ProjectionBatch,
) -> Result<(), ProjectionIssue> {
    let entries = result
        .get("data")
        .and_then(Value::as_array)
        .ok_or_else(|| ProjectionIssue::new(IssueKind::InvalidField, "/result/data"))?;
    for (index, entry) in entries.iter().enumerate() {
        let pointer = format!("/result/data/{index}/item");
        let source = evidence(batch, &pointer, EvidenceOrigin::RpcResponse);
        match history_snapshot(entry, scope, source) {
            Ok(item) => push_effect(batch, ProjectionEffect::Item(Box::new(item))),
            Err(mut issue) => {
                if !issue.json_pointer.starts_with("/result/")
                    && !issue.json_pointer.starts_with("/request/")
                {
                    issue = issue.prefixed(&format!("/result/data/{index}"));
                }
                batch.issues.push(issue);
            }
        }
    }
    Ok(())
}

fn evidence(batch: &ProjectionBatch, pointer: &str, origin: EvidenceOrigin) -> Evidence {
    Evidence {
        receipt: batch.receipt,
        json_pointer: pointer.into(),
        origin,
    }
}

fn push_effect(batch: &mut ProjectionBatch, effect: ProjectionEffect) {
    check_optional_thread_ids(batch, &effect);
    if let Some(pointer) = unknown_item_pointer(&effect) {
        batch
            .issues
            .push(ProjectionIssue::new(IssueKind::UnknownItemType, pointer));
    }
    batch.effects.push(effect);
}

fn unknown_item_pointer(effect: &ProjectionEffect) -> Option<String> {
    let ProjectionEffect::Item(item) = effect else {
        return None;
    };
    let ItemUpdate::Snapshot(snapshot) = &item.update else {
        return None;
    };
    (snapshot.category == ItemCategory::Unknown)
        .then(|| format!("{}/type", item.evidence.json_pointer))
}

fn classify_batch(batch: &mut ProjectionBatch) {
    if batch.issues.is_empty() {
        return;
    }
    batch.status = if !batch.effects.is_empty() {
        ProjectionStatus::Partial
    } else if batch.issues.iter().all(|issue| {
        matches!(
            issue.kind,
            IssueKind::UnknownMethod | IssueKind::UnknownItemType | IssueKind::RpcError
        )
    }) {
        ProjectionStatus::Unsupported
    } else {
        ProjectionStatus::Malformed
    };
}

fn check_envelope_associations(observation: &RuntimeObservation, batch: &mut ProjectionBatch) {
    batch.effects.retain(|effect| {
        let (thread, turn, item) = effect_associations(effect);
        let envelope = &observation.envelope;
        let valid = [
            (
                envelope.provider_session_id.as_deref(),
                effect_session_id(effect),
            ),
            (envelope.provider_thread_id.as_deref(), Some(thread)),
            (envelope.provider_turn_id.as_deref(), turn),
            (envelope.provider_item_id.as_deref(), item),
        ]
        .into_iter()
        .all(|(expected, actual)| match (expected, actual) {
            (Some(expected), Some(actual)) => expected == actual,
            _ => true,
        });
        if !valid {
            batch
                .issues
                .push(ProjectionIssue::new(IssueKind::ConflictingAssociation, ""));
        }
        valid
    });
}

fn effect_associations(effect: &ProjectionEffect) -> (&str, Option<&str>, Option<&str>) {
    match effect {
        ProjectionEffect::Item(item) => (
            &item.key.thread_id,
            Some(&item.key.turn_id),
            Some(&item.key.item_id),
        ),
        ProjectionEffect::Lifecycle(event) => (&event.thread_id, event.turn_id.as_deref(), None),
        ProjectionEffect::Signal(signal) => (
            &signal.thread_id,
            signal.turn_id.value().map(String::as_str),
            signal.item_id.value().map(String::as_str),
        ),
        ProjectionEffect::Usage(usage) => (&usage.thread_id, Some(&usage.turn_id), None),
        ProjectionEffect::Billing(estimate) => (&estimate.thread_id, None, None),
    }
}

fn effect_session_id(effect: &ProjectionEffect) -> Option<&str> {
    let ProjectionEffect::Lifecycle(lifecycle) = effect else {
        return None;
    };
    lifecycle
        .thread
        .as_ref()?
        .session_id
        .value()
        .map(String::as_str)
}

fn check_optional_thread_ids(batch: &mut ProjectionBatch, effect: &ProjectionEffect) {
    let ProjectionEffect::Lifecycle(lifecycle) = effect else {
        return;
    };
    let Some(thread) = &lifecycle.thread else {
        return;
    };
    for (key, value) in [
        ("sessionId", &thread.session_id),
        ("parentThreadId", &thread.parent_thread_id),
        ("forkedFromId", &thread.forked_from_id),
    ] {
        if matches!(value, Field::Invalid(_)) {
            batch.issues.push(ProjectionIssue::new(
                IssueKind::InvalidIdentifier,
                format!("{}/{key}", lifecycle.evidence.json_pointer),
            ));
        }
    }
}
