use serde_json::Value;

use super::protocol::{
    RoutingAudit, RpcScope, authenticated_method, response_scope_matches, reviewed_response_method,
};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub enum TrafficClass {
    #[default]
    Unknown,
    OrdinaryContent,
    Authentication,
}

#[derive(Debug, Clone, Copy, Default)]
pub struct IntakeContext<'a> {
    pub rpc_scope: Option<&'a RpcScope>,
    pub traffic_class: TrafficClass,
}

impl<'a> IntakeContext<'a> {
    pub fn new(rpc_scope: Option<&'a RpcScope>) -> Self {
        Self {
            rpc_scope,
            traffic_class: TrafficClass::Unknown,
        }
    }

    pub fn ordinary() -> Self {
        Self {
            rpc_scope: None,
            traffic_class: TrafficClass::OrdinaryContent,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExclusionReason {
    Authentication,
    UncorrelatedResponse,
    UnreviewedResponse,
    AmbiguousEnvelope,
    UnclassifiedTraffic,
}

pub(super) fn exclusion(
    value: Option<&Value>,
    context: IntakeContext<'_>,
    audit: RoutingAudit,
) -> Option<ExclusionReason> {
    if context.traffic_class == TrafficClass::Authentication
        || audit.authentication
        || context
            .rpc_scope
            .is_some_and(|scope| authenticated_method(&scope.method))
    {
        return Some(ExclusionReason::Authentication);
    }
    if audit.duplicate || (audit.method_present && audit.response_bearing) {
        return Some(ExclusionReason::AmbiguousEnvelope);
    }
    if context
        .rpc_scope
        .is_some_and(|scope| !reviewed_response_method(&scope.method))
    {
        return Some(ExclusionReason::UnreviewedResponse);
    }
    let Some(value) = value else {
        return unclassified(context);
    };
    if audit.response_bearing || (value.get("method").is_none() && value.get("id").is_some()) {
        return response_admission(value, context.rpc_scope);
    }
    if value
        .get("method")
        .and_then(Value::as_str)
        .is_some_and(reviewed_notification_method)
    {
        return None;
    }
    unclassified(context)
}

fn response_admission(value: &Value, scope: Option<&RpcScope>) -> Option<ExclusionReason> {
    if scope.is_some_and(|scope| response_scope_matches(value, scope)) {
        None
    } else {
        Some(ExclusionReason::UncorrelatedResponse)
    }
}

fn unclassified(context: IntakeContext<'_>) -> Option<ExclusionReason> {
    (context.traffic_class != TrafficClass::OrdinaryContent)
        .then_some(ExclusionReason::UnclassifiedTraffic)
}

fn reviewed_notification_method(method: &str) -> bool {
    matches!(
        method,
        "item/started"
            | "item/completed"
            | "item/agentMessage/delta"
            | "item/plan/delta"
            | "item/commandExecution/outputDelta"
            | "item/fileChange/outputDelta"
            | "item/reasoning/textDelta"
            | "item/reasoning/summaryTextDelta"
            | "thread/started"
            | "thread/status/changed"
            | "thread/closed"
            | "thread/archived"
            | "thread/unarchived"
            | "thread/deleted"
            | "thread/reverted"
            | "thread/tokenUsage/updated"
            | "turn/started"
            | "turn/completed"
            | "turn/plan/updated"
            | "turn/diff/updated"
            | "item/fileChange/patchUpdated"
            | "item/commandExecution/terminalInteraction"
            | "item/mcpToolCall/progress"
            | "item/tool/call"
            | "item/commandExecution/requestApproval"
            | "item/fileChange/requestApproval"
            | "item/permissions/requestApproval"
            | "item/tool/requestUserInput"
            | "item/reasoning/summaryPartAdded"
            | "model/rerouted"
            | "thread/compacted"
            | "error"
    )
}
