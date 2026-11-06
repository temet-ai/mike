use std::collections::{BTreeMap, HashMap, HashSet};

use chrono::{DateTime, Utc};
use serde::Serialize;

use super::export::MergedFlow;
use super::{dto, map};
use crate::store::persistence::session_flow as persist;

fn is_interval_row(activity: &persist::FlowActivity) -> bool {
    matches!(
        (&activity.kind, activity.source.table.as_str()),
        (persist::FlowComponent::Requests, "requests.requests")
            | (
                persist::FlowComponent::VerifierEvaluations,
                "steering.evaluations"
            )
            | (
                persist::FlowComponent::Consolidation,
                "memory.consolidation_runs"
            )
    )
}

const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;
const JS_SAFE_MASK: u64 = (1u64 << 53) - 1;

fn stable_id(input: &str) -> i64 {
    let hash = input.bytes().fold(FNV_OFFSET, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(FNV_PRIME)
    });
    (hash & JS_SAFE_MASK) as i64
}

fn track_for(actor_id: &str, kind: &persist::FlowActorKind) -> (i64, i64) {
    (
        stable_id(&format!("process:{kind:?}")),
        stable_id(&format!("thread:{actor_id}")),
    )
}

fn micros(time: DateTime<Utc>) -> i64 {
    time.timestamp_micros()
}

#[derive(Debug, Clone, Copy)]
struct Anchor {
    pid: i64,
    tid: i64,
    ts: i64,
}

#[derive(Debug, Serialize)]
pub(super) struct TraceEvent {
    name: String,
    cat: String,
    ph: &'static str,
    ts: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    dur: Option<i64>,
    pid: i64,
    tid: i64,
    #[serde(skip_serializing_if = "Option::is_none")]
    id: Option<i64>,
    args: serde_json::Value,
}

#[derive(Debug, Serialize)]
pub(super) struct PerfettoMetadata {
    session_id: String,
    availability: dto::SessionFlowAvailability,
    coverage: Vec<dto::SessionFlowCoverage>,
}

#[derive(Debug, Serialize)]
pub(crate) struct PerfettoTrace {
    #[serde(rename = "traceEvents")]
    trace_events: Vec<TraceEvent>,
    #[serde(rename = "displayTimeUnit")]
    display_time_unit: &'static str,
    #[serde(rename = "otherData")]
    other_data: PerfettoMetadata,
}

pub(super) fn build_trace(merged: MergedFlow) -> PerfettoTrace {
    let actor_kind = actor_kind_index(&merged.actors);
    let anchors = activity_anchors(&merged.activities, &actor_kind);
    let mut events = process_thread_events(&merged.actors);
    events.extend(activity_events(&merged.activities, &anchors));
    events.extend(flow_events(&merged.relationships, &anchors));
    events.sort_by_key(|event| event.ts);
    let availability = merged
        .availability
        .map(map::wire_availability)
        .unwrap_or(dto::SessionFlowAvailability::NoAdmittedStart);
    PerfettoTrace {
        trace_events: events,
        display_time_unit: "ns",
        other_data: PerfettoMetadata {
            session_id: merged.session.to_string(),
            availability,
            coverage: merged
                .coverage
                .into_iter()
                .map(map::wire_coverage)
                .collect(),
        },
    }
}

fn actor_kind_index(actors: &[persist::FlowActor]) -> HashMap<&str, &persist::FlowActorKind> {
    let mut index = HashMap::new();
    for actor in actors {
        index.entry(actor.id.as_str()).or_insert(&actor.kind);
    }
    index
}

fn activity_anchors(
    activities: &[persist::FlowActivity],
    actor_kind: &HashMap<&str, &persist::FlowActorKind>,
) -> HashMap<String, Anchor> {
    let mut anchors = HashMap::new();
    for activity in activities {
        let kind = actor_kind
            .get(activity.actor_id.as_str())
            .copied()
            .unwrap_or(&persist::FlowActorKind::Session);
        let (pid, tid) = track_for(&activity.actor_id, kind);
        let ts = micros(activity.occurred_at.unwrap_or(activity.recorded_at));
        anchors.insert(activity.id.clone(), Anchor { pid, tid, ts });
    }
    anchors
}

fn process_thread_events(actors: &[persist::FlowActor]) -> Vec<TraceEvent> {
    let mut seen = HashSet::new();
    let mut events = Vec::new();
    for actor in actors {
        let (pid, tid) = track_for(&actor.id, &actor.kind);
        if seen.insert((pid, tid)) {
            events.push(metadata_event(
                pid,
                tid,
                "process_name",
                &format!("{:?}", actor.kind),
            ));
            events.push(metadata_event(pid, tid, "thread_name", &actor.label));
        }
    }
    events
}

fn metadata_event(pid: i64, tid: i64, name: &'static str, label: &str) -> TraceEvent {
    TraceEvent {
        name: name.into(),
        cat: "__metadata".into(),
        ph: "M",
        ts: 0,
        dur: None,
        pid,
        tid,
        id: None,
        args: serde_json::json!({ "name": label }),
    }
}

fn activity_events(
    activities: &[persist::FlowActivity],
    anchors: &HashMap<String, Anchor>,
) -> Vec<TraceEvent> {
    let mut intervals: BTreeMap<(String, String), Vec<&persist::FlowActivity>> = BTreeMap::new();
    let mut events = Vec::new();
    for activity in activities {
        if is_interval_row(activity) {
            intervals
                .entry((
                    activity.source.table.clone(),
                    activity.source.row_id.to_string(),
                ))
                .or_default()
                .push(activity);
        } else if let Some(&anchor) = anchors.get(&activity.id) {
            events.push(mark_event(activity, anchor));
        }
    }
    for members in intervals.into_values() {
        if let Some(event) = interval_event(&members, anchors) {
            events.push(event);
        }
    }
    events
}

fn mark_event(activity: &persist::FlowActivity, anchor: Anchor) -> TraceEvent {
    TraceEvent {
        name: activity.phase.clone(),
        cat: activity.source.table.clone(),
        ph: "i",
        ts: anchor.ts,
        dur: None,
        pid: anchor.pid,
        tid: anchor.tid,
        id: None,
        args: activity_args(activity),
    }
}

fn interval_event(
    members: &[&persist::FlowActivity],
    anchors: &HashMap<String, Anchor>,
) -> Option<TraceEvent> {
    let (first, rest) = members.split_first()?;
    let first_anchor = *anchors.get(&first.id)?;
    let mut start = first_anchor.ts;
    let mut end = start;
    for member in rest {
        let anchor = anchors.get(&member.id)?;
        start = start.min(anchor.ts);
        end = end.max(anchor.ts);
    }
    let name = format!(
        "{} {}",
        first.source.table,
        members
            .iter()
            .map(|m| m.phase.as_str())
            .collect::<Vec<_>>()
            .join("\u{2192}")
    );
    let args = interval_args(members);
    Some(if members.len() == 1 {
        TraceEvent {
            name,
            cat: first.source.table.clone(),
            ph: "B",
            ts: start,
            dur: None,
            pid: first_anchor.pid,
            tid: first_anchor.tid,
            id: None,
            args,
        }
    } else if end == start {
        TraceEvent {
            name,
            cat: first.source.table.clone(),
            ph: "i",
            ts: start,
            dur: None,
            pid: first_anchor.pid,
            tid: first_anchor.tid,
            id: None,
            args,
        }
    } else {
        TraceEvent {
            name,
            cat: first.source.table.clone(),
            ph: "X",
            ts: start,
            dur: Some(end - start),
            pid: first_anchor.pid,
            tid: first_anchor.tid,
            id: None,
            args,
        }
    })
}

fn interval_args(members: &[&persist::FlowActivity]) -> serde_json::Value {
    let mut merged = serde_json::Map::new();
    for member in members {
        if let serde_json::Value::Object(map) = activity_args(member) {
            for (key, value) in map {
                if !value.is_null() {
                    merged.insert(key, value);
                }
            }
        }
    }
    serde_json::Value::Object(merged)
}

fn activity_args(activity: &persist::FlowActivity) -> serde_json::Value {
    let metadata = map::wire_metadata(activity.metadata.clone());
    let mut value = serde_json::to_value(metadata).unwrap_or(serde_json::Value::Null);
    if let serde_json::Value::Object(fields) = &mut value {
        fields.insert(
            "lifecycle".into(),
            serde_json::json!(map::wire_lifecycle(&activity.lifecycle)),
        );
        fields.insert(
            "time_semantics".into(),
            serde_json::json!(map::wire_time_semantics(&activity.time_semantics)),
        );
        if let Some(outcome) = &activity.outcome {
            fields.insert(
                "outcome".into(),
                serde_json::json!(map::wire_outcome(outcome)),
            );
        }
        fields.insert(
            "source_row_id".into(),
            serde_json::json!(activity.source.row_id.to_string()),
        );
        if let Some(request_id) = activity.source.request_id {
            fields.insert(
                "request_id".into(),
                serde_json::json!(request_id.to_string()),
            );
        }
    }
    value
}

fn flow_events(
    relationships: &[persist::FlowRelationship],
    anchors: &HashMap<String, Anchor>,
) -> Vec<TraceEvent> {
    let mut events = Vec::new();
    for relationship in relationships {
        let (Some(from), Some(to)) = (&relationship.from_id, &relationship.to_id) else {
            continue;
        };
        let (Some(&from_anchor), Some(&to_anchor)) = (anchors.get(from), anchors.get(to)) else {
            continue;
        };
        let flow_id = stable_id(&relationship.id);
        events.push(flow_event("s", &relationship.phase, from_anchor, flow_id));
        events.push(flow_event("f", &relationship.phase, to_anchor, flow_id));
    }
    events
}

fn flow_event(ph: &'static str, name: &str, anchor: Anchor, flow_id: i64) -> TraceEvent {
    TraceEvent {
        name: name.into(),
        cat: "flow".into(),
        ph,
        ts: anchor.ts,
        dur: None,
        pid: anchor.pid,
        tid: anchor.tid,
        id: Some(flow_id),
        args: serde_json::json!({}),
    }
}
