use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::export::MergedFlow;
use super::export_trace as trace;
use crate::store::persistence::session_flow as persist;

fn dt(seconds: i64) -> anyhow::Result<DateTime<Utc>> {
    DateTime::from_timestamp(1_700_000_000 + seconds, 0)
        .ok_or_else(|| anyhow::anyhow!("out-of-range fixture timestamp"))
}

fn source(table: &str, row_id: persist::FlowRowId) -> persist::FlowSource {
    persist::FlowSource {
        table: table.into(),
        row_id,
        session_id: None,
        request_id: None,
        detail_href: None,
    }
}

fn activity(
    kind: persist::FlowComponent,
    table: &str,
    row_id: i64,
    phase: &str,
    actor_id: &str,
    at: DateTime<Utc>,
) -> persist::FlowActivity {
    persist::FlowActivity {
        id: format!("{table}:{row_id}:{phase}"),
        phase: phase.into(),
        actor_id: actor_id.into(),
        source: source(table, persist::FlowRowId::Bigint(row_id)),
        kind,
        occurred_at: Some(at),
        recorded_at: at,
        ended_at: None,
        time_semantics: persist::FlowTimeSemantics::RecordedObservation,
        lifecycle: persist::FlowLifecycle::Observed,
        outcome: None,
        source_revision: None,
        metadata: persist::FlowMetadata::default(),
    }
}

fn actor(id: &str, kind: persist::FlowActorKind, label: &str) -> persist::FlowActor {
    persist::FlowActor {
        id: id.into(),
        kind,
        session_id: None,
        label: label.into(),
        producer_id: None,
        provenance: persist::FlowProvenance::SourceRow,
        parent_actor_id: None,
        source: None,
    }
}

fn merged(
    session: Uuid,
    actors: Vec<persist::FlowActor>,
    activities: Vec<persist::FlowActivity>,
) -> MergedFlow {
    let mut merged = MergedFlow::new(session);
    merged.actors = actors;
    merged.activities = activities;
    merged
}

fn events_of(built: &trace::PerfettoTrace) -> anyhow::Result<Vec<serde_json::Value>> {
    let value = serde_json::to_value(built)?;
    let events = value
        .get("traceEvents")
        .and_then(|events| events.as_array())
        .cloned()
        .unwrap_or_default();
    Ok(events
        .into_iter()
        .filter(|e| e["cat"] != "__metadata")
        .collect())
}

#[test]
fn unfinished_interval_stays_open_with_no_matching_end() -> anyhow::Result<()> {
    let session = Uuid::new_v4();
    let actors = vec![actor(
        "session:x",
        persist::FlowActorKind::Session,
        "Session",
    )];
    let activities = vec![activity(
        persist::FlowComponent::Requests,
        "requests.requests",
        5,
        "started",
        "session:x",
        dt(0)?,
    )];
    let built = trace::build_trace(merged(session, actors, activities));
    let events = events_of(&built)?;
    assert_eq!(events.len(), 1, "one open interval, no synthesized close");
    assert_eq!(events[0]["ph"], "B");
    assert!(
        events[0].get("dur").is_none(),
        "an open interval has no duration"
    );
    Ok(())
}

#[test]
fn tied_start_and_end_collapses_to_a_point() -> anyhow::Result<()> {
    let session = Uuid::new_v4();
    let at = dt(10)?;
    let actors = vec![actor(
        "session:x",
        persist::FlowActorKind::Session,
        "Session",
    )];
    let activities = vec![
        activity(
            persist::FlowComponent::Requests,
            "requests.requests",
            7,
            "started",
            "session:x",
            at,
        ),
        activity(
            persist::FlowComponent::Requests,
            "requests.requests",
            7,
            "completed",
            "session:x",
            at,
        ),
    ];
    let built = trace::build_trace(merged(session, actors, activities));
    let events = events_of(&built)?;
    assert_eq!(
        events.len(),
        1,
        "same start/end merges into one point, not two events"
    );
    assert_eq!(events[0]["ph"], "i");
    assert!(events[0].get("dur").is_none());
    Ok(())
}

#[test]
fn closed_interval_carries_the_measured_duration() -> anyhow::Result<()> {
    let session = Uuid::new_v4();
    let actors = vec![actor(
        "session:x",
        persist::FlowActorKind::Session,
        "Session",
    )];
    let activities = vec![
        activity(
            persist::FlowComponent::Requests,
            "requests.requests",
            9,
            "started",
            "session:x",
            dt(0)?,
        ),
        activity(
            persist::FlowComponent::Requests,
            "requests.requests",
            9,
            "completed",
            "session:x",
            dt(3)?,
        ),
    ];
    let built = trace::build_trace(merged(session, actors, activities));
    let events = events_of(&built)?;
    assert_eq!(events.len(), 1);
    assert_eq!(events[0]["ph"], "X");
    assert_eq!(events[0]["dur"], serde_json::json!(3_000_000));
    Ok(())
}

#[test]
fn non_interval_component_never_merges_its_phases() -> anyhow::Result<()> {
    let session = Uuid::new_v4();
    let actor_id = "mike:x:steering.interventions";
    let actors = vec![actor(actor_id, persist::FlowActorKind::Mike, "Mike")];
    let activities = vec![
        activity(
            persist::FlowComponent::Interventions,
            "steering.interventions",
            3,
            "fired",
            actor_id,
            dt(0)?,
        ),
        activity(
            persist::FlowComponent::Interventions,
            "steering.interventions",
            3,
            "attempted",
            actor_id,
            dt(1)?,
        ),
    ];
    let built = trace::build_trace(merged(session, actors, activities));
    let events = events_of(&built)?;
    assert_eq!(
        events.len(),
        2,
        "each phase of a non-interval source stays its own mark"
    );
    assert!(events.iter().all(|e| e["ph"] == "i"));
    Ok(())
}

#[test]
fn source_count_and_time_parity_across_mixed_activities() -> anyhow::Result<()> {
    let session = Uuid::new_v4();
    let actor_id = "session:x";
    let actors = vec![actor(actor_id, persist::FlowActorKind::Session, "Session")];
    let activities = vec![
        activity(
            persist::FlowComponent::Requests,
            "requests.requests",
            1,
            "started",
            actor_id,
            dt(0)?,
        ),
        activity(
            persist::FlowComponent::Requests,
            "requests.requests",
            1,
            "completed",
            actor_id,
            dt(5)?,
        ),
        activity(
            persist::FlowComponent::Tools,
            "tools.uses",
            2,
            "observed",
            actor_id,
            dt(1)?,
        ),
        activity(
            persist::FlowComponent::Results,
            "tools.results",
            3,
            "observed",
            actor_id,
            dt(2)?,
        ),
    ];
    let built = trace::build_trace(merged(session, actors, activities));
    let events = events_of(&built)?;
    assert_eq!(events.len(), 3);
    let request_event = events
        .iter()
        .find(|e| e["cat"] == "requests.requests")
        .ok_or_else(|| anyhow::anyhow!("missing request interval event"))?;
    assert_eq!(
        request_event["ts"],
        serde_json::json!(dt(0)?.timestamp_micros())
    );
    assert_eq!(request_event["dur"], serde_json::json!(5_000_000));
    let tool_event = events
        .iter()
        .find(|e| e["cat"] == "tools.uses")
        .ok_or_else(|| anyhow::anyhow!("missing tool mark event"))?;
    assert_eq!(
        tool_event["ts"],
        serde_json::json!(dt(1)?.timestamp_micros())
    );
    Ok(())
}

#[test]
fn flow_arrow_only_for_a_resolvable_explicit_join() -> anyhow::Result<()> {
    let session = Uuid::new_v4();
    let actor_id = "session:x";
    let actors = vec![actor(actor_id, persist::FlowActorKind::Session, "Session")];
    let activities = vec![
        activity(
            persist::FlowComponent::Tools,
            "tools.uses",
            1,
            "observed",
            actor_id,
            dt(0)?,
        ),
        activity(
            persist::FlowComponent::Results,
            "tools.results",
            2,
            "observed",
            actor_id,
            dt(1)?,
        ),
    ];
    let mut merged = merged(session, actors, activities);
    merged.relationships = vec![
        persist::FlowRelationship {
            id: "tools.results:2:carrier".into(),
            from_id: Some("tools.uses:1:observed".into()),
            to_id: Some("tools.results:2:observed".into()),
            kind: persist::FlowRelationshipKind::ResultCarrier,
            source: source("tools.results", persist::FlowRowId::Bigint(2)),
            phase: "observed".into(),
            occurred_at: Some(dt(1)?),
            recorded_at: dt(1)?,
        },
        persist::FlowRelationship {
            id: "mailbox.messages:9:sender".into(),
            from_id: Some("session:unresolved-actor".into()),
            to_id: Some("tools.results:2:observed".into()),
            kind: persist::FlowRelationshipKind::MessageSender,
            source: source("mailbox.messages", persist::FlowRowId::Bigint(9)),
            phase: "queued".into(),
            occurred_at: Some(dt(0)?),
            recorded_at: dt(0)?,
        },
    ];
    let built = trace::build_trace(merged);
    let flow_events: Vec<_> = events_of(&built)?
        .into_iter()
        .filter(|e| e["cat"] == "flow")
        .collect();
    assert_eq!(
        flow_events.len(),
        2,
        "only the resolvable relationship (both ends are real activity ids) draws an arrow"
    );
    assert_eq!(flow_events[0]["ph"], "s");
    assert_eq!(flow_events[1]["ph"], "f");
    assert_eq!(flow_events[0]["id"], flow_events[1]["id"]);
    Ok(())
}

#[test]
fn escaping_and_a_javascript_unsafe_id_round_trip() -> anyhow::Result<()> {
    let session = Uuid::new_v4();
    let actor_id = "session:x";
    let a = actor(
        actor_id,
        persist::FlowActorKind::Session,
        "Wei\u{2019}rd \"label\"\\ 名前",
    );
    let big_row_id = 9_007_199_254_740_993_i64;
    let activities = vec![activity(
        persist::FlowComponent::Tools,
        "tools.uses",
        big_row_id,
        "observed",
        actor_id,
        dt(0)?,
    )];
    let built = trace::build_trace(merged(session, vec![a], activities));
    let json = serde_json::to_string(&built)?;
    let round_tripped: serde_json::Value = serde_json::from_str(&json)?;
    let all_events = round_tripped
        .get("traceEvents")
        .and_then(|events| events.as_array())
        .cloned()
        .unwrap_or_default();
    let mark = all_events
        .iter()
        .find(|e| e["cat"] == "tools.uses")
        .ok_or_else(|| anyhow::anyhow!("missing mark event"))?;
    assert_eq!(mark["args"]["source_row_id"], big_row_id.to_string());
    let pid = mark["pid"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("pid not a number"))?;
    let tid = mark["tid"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("tid not a number"))?;
    assert!((0..(1i64 << 53)).contains(&pid));
    assert!((0..(1i64 << 53)).contains(&tid));
    let thread_name = all_events
        .iter()
        .find(|e| e["name"] == "thread_name")
        .ok_or_else(|| anyhow::anyhow!("missing thread_name metadata event"))?;
    assert_eq!(
        thread_name["args"]["name"],
        "Wei\u{2019}rd \"label\"\\ 名前"
    );
    Ok(())
}

#[test]
fn raw_sentinel_text_never_appears_in_export_args() -> anyhow::Result<()> {
    let session = Uuid::new_v4();
    let mut activity_row = activity(
        persist::FlowComponent::LlmCalls,
        "llm.calls",
        1,
        "recorded",
        "mike:x:llm.calls",
        dt(0)?,
    );
    activity_row.metadata.role = Some("RAW_SENTINEL_ROLE".into());
    let actors = vec![actor(
        "mike:x:llm.calls",
        persist::FlowActorKind::Mike,
        "Mike",
    )];
    let built = trace::build_trace(merged(session, actors, vec![activity_row]));
    let json = serde_json::to_string(&built)?;
    assert!(
        json.contains("RAW_SENTINEL_ROLE"),
        "the declared field itself is legitimate output"
    );
    assert!(!json.contains("request_body"));
    assert!(!json.contains("response_body"));
    assert!(!json.contains("human_prompt"));
    Ok(())
}
