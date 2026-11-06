use super::*;
use std::collections::HashSet;

impl std::fmt::Display for FlowRowId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Bigint(id) => id.fmt(f),
            Self::Uuid(id) => id.fmt(f),
        }
    }
}

pub fn empty_page(
    session: Uuid,
    component: FlowComponent,
    availability: FlowAvailability,
) -> FlowPage {
    FlowPage {
        version: 1,
        session_id: session,
        availability,
        actors: vec![],
        activities: vec![],
        relationships: vec![],
        next_cursor: None,
        coverage: FlowCoverage {
            component,
            lower_id: None,
            upper_id: None,
            returned_through_id: None,
            exhausted: true,
            retention: FlowRetention::Unknown,
            capture_complete: None,
            latest_only: false,
            best_effort: true,
            omitted_components: vec![],
            server_observed_at: Utc::now(),
            stale_reason: None,
        },
    }
}

pub(super) fn source(table: &str, id: i64, session: Uuid, request: Option<i64>) -> FlowSource {
    FlowSource {
        table: table.into(),
        row_id: FlowRowId::Bigint(id),
        session_id: Some(session),
        request_id: request,
        detail_href: request.map(|id| format!("/api/requests/{id}")),
    }
}

pub fn vocabulary(value: Option<String>) -> Option<String> {
    value.filter(|v| {
        !v.is_empty()
            && v.len() <= 128
            && v.chars()
                .all(|c| c.is_ascii_alphanumeric() || "_:-./".contains(c))
    })
}

pub(super) fn actor(
    session: Uuid,
    kind: FlowActorKind,
    id: String,
    label: String,
    source: Option<FlowSource>,
) -> FlowActor {
    FlowActor {
        id,
        kind,
        session_id: Some(session),
        label,
        producer_id: None,
        provenance: FlowProvenance::SourceRow,
        parent_actor_id: None,
        source,
    }
}

pub(super) fn request_actor(
    session: Uuid,
    request: i64,
    producer: Option<String>,
    agent_instance_id: Option<i64>,
    origin: &str,
    source: &FlowSource,
) -> FlowActor {
    if let Some(producer) = producer.filter(|p| !p.is_empty()) {
        let mut actor = actor(
            session,
            FlowActorKind::Worker,
            format!("worker:{session}:{producer}"),
            "Worker".into(),
            Some(source.clone()),
        );
        actor.producer_id = Some(producer);
        actor.provenance = FlowProvenance::ProducerHeader;
        actor
    } else if matches!(origin, "subagent" | "workflow_subagent") {
        let mut actor = actor(
            session,
            FlowActorKind::Worker,
            format!("unresolved-worker:{session}:{request}"),
            "Unresolved worker".into(),
            Some(source.clone()),
        );
        actor.provenance = if agent_instance_id.is_some() {
            FlowProvenance::Heuristic
        } else {
            FlowProvenance::Unknown
        };
        actor
    } else if matches!(
        origin,
        "title_sidecar"
            | "advisor_sidecar"
            | "quota_probe"
            | "monitor_sidecar"
            | "count_tokens_sidecar"
            | "web_fetch_sidecar"
            | "next_input_suggestion_sidecar"
    ) {
        actor(
            session,
            FlowActorKind::Sidecar,
            format!("sidecar:{session}:{origin}"),
            format!("Sidecar {origin}"),
            Some(source.clone()),
        )
    } else {
        actor(
            session,
            FlowActorKind::Session,
            format!("session:{session}"),
            "Session".into(),
            Some(source.clone()),
        )
    }
}

pub(super) fn activity(
    source: FlowSource,
    phase: &str,
    actor_id: String,
    kind: FlowComponent,
    time: DateTime<Utc>,
) -> FlowActivity {
    FlowActivity {
        id: format!("{}:{}:{phase}", source.table, source.row_id),
        phase: phase.into(),
        actor_id,
        source,
        kind,
        occurred_at: Some(time),
        recorded_at: time,
        ended_at: None,
        time_semantics: FlowTimeSemantics::RecordedObservation,
        lifecycle: FlowLifecycle::Observed,
        outcome: None,
        source_revision: None,
        metadata: FlowMetadata::default(),
    }
}

pub(super) fn relationship(
    activity: &FlowActivity,
    kind: FlowRelationshipKind,
    suffix: &str,
    from_id: Option<String>,
    to_id: Option<String>,
) -> FlowRelationship {
    FlowRelationship {
        id: format!("{}:{suffix}", activity.id),
        from_id,
        to_id,
        kind,
        source: activity.source.clone(),
        phase: activity.phase.clone(),
        occurred_at: activity.occurred_at,
        recorded_at: activity.recorded_at,
    }
}

pub(super) fn finish(page: &mut FlowPage) {
    let mut actors = HashSet::new();
    let mut peer_count = 0;
    let mut omitted = false;
    page.actors.retain(|actor| {
        if !actors.insert(actor.id.clone()) {
            return false;
        }
        if actor.kind == FlowActorKind::Peer {
            peer_count += 1;
            if peer_count > 128 {
                omitted = true;
                return false;
            }
        }
        true
    });
    if omitted {
        page.coverage.stale_reason =
            Some("peer_actor_limit: endpoint IDs retained; expand scope separately".into());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn metadata_labels_omit_oversized_values_and_ids_remain_full() {
        assert!(vocabulary(Some("a".repeat(129))).is_none());
        assert!(vocabulary(Some("raw words with spaces".into())).is_none());
        assert_eq!(
            vocabulary(Some("mcp__tools__name".into())),
            Some("mcp__tools__name".into())
        );
        assert_eq!(
            FlowRowId::Bigint(9_007_199_254_740_993).to_string(),
            "9007199254740993"
        );
    }
}
