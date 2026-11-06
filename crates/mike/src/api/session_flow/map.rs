use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::dto;
use crate::store::persistence::session_flow as persist;

pub(super) fn persist_component(component: &dto::SessionFlowComponent) -> persist::FlowComponent {
    use dto::SessionFlowComponent as W;
    use persist::FlowComponent as P;
    match component {
        W::Requests => P::Requests,
        W::Dispatches => P::Dispatches,
        W::Tools => P::Tools,
        W::Results => P::Results,
        W::Messages => P::Messages,
        W::WorkerStatus => P::WorkerStatus,
        W::ManifestStages => P::ManifestStages,
        W::LessonInjections => P::LessonInjections,
        W::VerifierEvaluations => P::VerifierEvaluations,
        W::Judgments => P::Judgments,
        W::LlmCalls => P::LlmCalls,
        W::Embeddings => P::Embeddings,
        W::Consolidation => P::Consolidation,
        W::Interventions => P::Interventions,
    }
}

fn wire_component(component: &persist::FlowComponent) -> dto::SessionFlowComponent {
    use dto::SessionFlowComponent as W;
    use persist::FlowComponent as P;
    match component {
        P::Requests => W::Requests,
        P::Dispatches => W::Dispatches,
        P::Tools => W::Tools,
        P::Results => W::Results,
        P::Messages => W::Messages,
        P::WorkerStatus => W::WorkerStatus,
        P::ManifestStages => W::ManifestStages,
        P::LessonInjections => W::LessonInjections,
        P::VerifierEvaluations => W::VerifierEvaluations,
        P::Judgments => W::Judgments,
        P::LlmCalls => W::LlmCalls,
        P::Embeddings => W::Embeddings,
        P::Consolidation => W::Consolidation,
        P::Interventions => W::Interventions,
    }
}

fn wire_actor_kind(kind: &persist::FlowActorKind) -> dto::SessionFlowActorKind {
    use dto::SessionFlowActorKind as W;
    use persist::FlowActorKind as P;
    match kind {
        P::Session => W::Session,
        P::Worker => W::Worker,
        P::UnresolvedDispatch => W::UnresolvedDispatch,
        P::Sidecar => W::Sidecar,
        P::Peer => W::Peer,
        P::Upstream => W::Upstream,
        P::ToolService => W::ToolService,
        P::Mike => W::Mike,
    }
}

fn wire_provenance(provenance: &persist::FlowProvenance) -> dto::SessionFlowProvenance {
    use dto::SessionFlowProvenance as W;
    use persist::FlowProvenance as P;
    match provenance {
        P::ProducerHeader => W::ProducerHeader,
        P::ExplicitCorrelation => W::ExplicitCorrelation,
        P::SourceRow => W::SourceRow,
        P::Heuristic => W::Heuristic,
        P::Unknown => W::Unknown,
    }
}

pub(super) fn wire_time_semantics(
    value: &persist::FlowTimeSemantics,
) -> dto::SessionFlowTimeSemantics {
    use dto::SessionFlowTimeSemantics as W;
    use persist::FlowTimeSemantics as P;
    match value {
        P::CaptureInterval => W::CaptureInterval,
        P::RecordedObservation => W::RecordedObservation,
        P::StoredOutput => W::StoredOutput,
        P::MeasuredLatency => W::MeasuredLatency,
        P::Unknown => W::Unknown,
    }
}

pub(super) fn wire_lifecycle(value: &persist::FlowLifecycle) -> dto::SessionFlowLifecycle {
    use dto::SessionFlowLifecycle as W;
    use persist::FlowLifecycle as P;
    match value {
        P::Started => W::Started,
        P::Observed => W::Observed,
        P::Completed => W::Completed,
        P::Queued => W::Queued,
        P::Claimed => W::Claimed,
        P::Attempted => W::Attempted,
        P::Delivered => W::Delivered,
        P::Consumed => W::Consumed,
        P::Expired => W::Expired,
        P::Reverted => W::Reverted,
        P::Unknown => W::Unknown,
    }
}

pub(super) fn wire_outcome(value: &persist::FlowOutcome) -> dto::SessionFlowOutcome {
    use dto::SessionFlowOutcome as W;
    use persist::FlowOutcome as P;
    match value {
        P::Success => W::Success,
        P::Error => W::Error,
        P::Applied => W::Applied,
        P::Unchanged => W::Unchanged,
        P::Skipped => W::Skipped,
        P::FailedClosed => W::FailedClosed,
        P::Expired => W::Expired,
        P::Reverted => W::Reverted,
        P::Resolved => W::Resolved,
        P::Unknown => W::Unknown,
    }
}

fn wire_relationship_kind(
    value: &persist::FlowRelationshipKind,
) -> dto::SessionFlowRelationshipKind {
    use dto::SessionFlowRelationshipKind as W;
    use persist::FlowRelationshipKind as P;
    match value {
        P::DispatchTool => W::DispatchTool,
        P::ResultCarrier => W::ResultCarrier,
        P::MessageSender => W::MessageSender,
        P::MessageRecipient => W::MessageRecipient,
        P::MessageReply => W::MessageReply,
        P::ExplicitParent => W::ExplicitParent,
        P::RequestTool => W::RequestTool,
        P::ActivityRequest => W::ActivityRequest,
    }
}

fn wire_retention(value: &persist::FlowRetention) -> dto::SessionFlowRetention {
    use dto::SessionFlowRetention as W;
    use persist::FlowRetention as P;
    match value {
        P::Retained => W::Retained,
        P::Partial => W::Partial,
        P::Unknown => W::Unknown,
    }
}

fn wire_timestamp(value: DateTime<Utc>) -> String {
    value.to_rfc3339()
}

fn wire_row_id(id: &persist::FlowRowId) -> String {
    id.to_string()
}

fn wire_source(source: &persist::FlowSource) -> dto::SessionFlowSource {
    dto::SessionFlowSource {
        table: source.table.clone(),
        row_id: source.row_id.to_string(),
        session_id: source.session_id.map(|id| id.to_string()),
        request_id: source.request_id.map(|id| id.to_string()),
        detail_href: source.detail_href.clone(),
    }
}

fn wire_actor(actor: persist::FlowActor) -> dto::SessionFlowActor {
    dto::SessionFlowActor {
        id: actor.id,
        kind: wire_actor_kind(&actor.kind),
        session_id: actor.session_id.map(|id| id.to_string()),
        label: actor.label,
        producer_id: actor.producer_id,
        provenance: wire_provenance(&actor.provenance),
        parent_actor_id: actor.parent_actor_id,
        source: actor.source.as_ref().map(wire_source),
    }
}

pub(super) fn wire_metadata(metadata: persist::FlowMetadata) -> dto::SessionFlowMetadata {
    dto::SessionFlowMetadata {
        tool_name: metadata.tool_name,
        wire_tool_use_id: metadata.wire_tool_use_id,
        status_code: metadata.status_code,
        model: metadata.model,
        latency_ms: metadata.latency_ms,
        input_tokens: metadata.input_tokens.map(|v| v.to_string()),
        output_tokens: metadata.output_tokens.map(|v| v.to_string()),
        stage: metadata.stage,
        role: metadata.role,
        steerer_id: metadata.steerer_id.map(|v| v.to_string()),
        turn_id: metadata.turn_id.map(|v| v.to_string()),
        cost_usd: metadata.cost_usd,
        scope: metadata.scope,
        verdict: metadata.verdict,
        has_verdict: metadata.has_verdict,
        attempt: metadata.attempt,
        body_available: metadata.body_available,
        heuristic_instance_id: metadata.heuristic_instance_id.map(|v| v.to_string()),
        agent_name: metadata.agent_name,
        dispatch_description: metadata.dispatch_description,
        firing_rule_count: metadata.firing_rule_count,
    }
}

fn wire_activity(activity: persist::FlowActivity) -> dto::SessionFlowActivity {
    dto::SessionFlowActivity {
        id: activity.id,
        phase: activity.phase,
        actor_id: activity.actor_id,
        source: wire_source(&activity.source),
        kind: wire_component(&activity.kind),
        occurred_at: activity.occurred_at.map(wire_timestamp),
        recorded_at: wire_timestamp(activity.recorded_at),
        ended_at: activity.ended_at.map(wire_timestamp),
        time_semantics: wire_time_semantics(&activity.time_semantics),
        lifecycle: wire_lifecycle(&activity.lifecycle),
        outcome: activity.outcome.as_ref().map(wire_outcome),
        source_revision: activity.source_revision,
        metadata: wire_metadata(activity.metadata),
    }
}

fn wire_relationship(relationship: persist::FlowRelationship) -> dto::SessionFlowRelationship {
    dto::SessionFlowRelationship {
        id: relationship.id,
        from_id: relationship.from_id,
        to_id: relationship.to_id,
        kind: wire_relationship_kind(&relationship.kind),
        source: wire_source(&relationship.source),
        phase: relationship.phase,
        occurred_at: relationship.occurred_at.map(wire_timestamp),
        recorded_at: wire_timestamp(relationship.recorded_at),
    }
}

pub(super) fn wire_availability(
    availability: persist::FlowAvailability,
) -> dto::SessionFlowAvailability {
    match availability {
        persist::FlowAvailability::Eligible {
            capture_version,
            admitted_at,
            producer,
        } => dto::SessionFlowAvailability::Eligible {
            capture_version,
            admitted_at: wire_timestamp(admitted_at),
            producer,
        },
        persist::FlowAvailability::UnsupportedVersion { capture_version } => {
            dto::SessionFlowAvailability::UnsupportedVersion { capture_version }
        }
        persist::FlowAvailability::NoAdmittedStart => dto::SessionFlowAvailability::NoAdmittedStart,
    }
}

pub(super) fn wire_coverage(coverage: persist::FlowCoverage) -> dto::SessionFlowCoverage {
    dto::SessionFlowCoverage {
        component: wire_component(&coverage.component),
        lower_id: coverage.lower_id.as_ref().map(wire_row_id),
        upper_id: coverage.upper_id.as_ref().map(wire_row_id),
        returned_through_id: coverage.returned_through_id.as_ref().map(wire_row_id),
        exhausted: coverage.exhausted,
        retention: wire_retention(&coverage.retention),
        capture_complete: coverage.capture_complete,
        latest_only: coverage.latest_only,
        best_effort: coverage.best_effort,
        omitted_components: coverage
            .omitted_components
            .iter()
            .map(wire_component)
            .collect(),
        server_observed_at: wire_timestamp(coverage.server_observed_at),
        stale_reason: coverage.stale_reason,
    }
}

fn wire_cursor(cursor: &persist::FlowCursor) -> dto::SessionFlowCursor {
    dto::SessionFlowCursor {
        version: cursor.version,
        session_id: cursor.session_id.to_string(),
        component: wire_component(&cursor.component),
        filter: cursor.filter.clone(),
        after_id: cursor.after_id.to_string(),
        upper_id: cursor.upper_id.to_string(),
    }
}

pub(super) fn persist_cursor(cursor: &dto::SessionFlowCursor) -> Option<persist::FlowCursor> {
    Some(persist::FlowCursor {
        version: cursor.version,
        session_id: Uuid::parse_str(&cursor.session_id).ok()?,
        component: persist_component(&cursor.component),
        filter: cursor.filter.clone(),
        after_id: cursor.after_id.parse().ok()?,
        upper_id: cursor.upper_id.parse().ok()?,
    })
}

pub(super) fn wire_page(
    page: persist::FlowPage,
) -> Result<dto::SessionFlowPage, serde_json::Error> {
    let next_cursor = page
        .next_cursor
        .as_ref()
        .map(wire_cursor)
        .map(|c| serde_json::to_string(&c))
        .transpose()?;
    let persist::FlowPage {
        version,
        session_id,
        availability,
        actors,
        activities,
        relationships,
        coverage,
        next_cursor: _,
    } = page;
    Ok(dto::SessionFlowPage {
        version,
        session_id: session_id.to_string(),
        availability: wire_availability(availability),
        actors: actors.into_iter().map(wire_actor).collect(),
        activities: activities.into_iter().map(wire_activity).collect(),
        relationships: relationships.into_iter().map(wire_relationship).collect(),
        coverage: wire_coverage(coverage),
        next_cursor,
    })
}
