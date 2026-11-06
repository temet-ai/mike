use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::SeatFacts;
use crate::launch::fleet::events::{EventKind, FleetEvent};

pub const STOP_KILL: &str = "stop kill";
pub const UNCONFIRMED_PREFIX: &str = "no first request within";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Observed {
    pub last_seen: Option<DateTime<Utc>>,
    pub unread_mail: bool,
    pub process_gone: bool,
    pub stopping: bool,
    pub confirmed_by_request: bool,
}

fn launch_id(event: &FleetEvent) -> i64 {
    event.payload["launch_id"].as_i64().unwrap_or(0)
}

fn about(event: &FleetEvent, task: &str, session: Uuid) -> bool {
    event.session_uuid == Some(session)
        || (event.session_uuid.is_none() && event.task.as_deref() == Some(task))
}

fn relaunches(events: &[FleetEvent], task: &str) -> u32 {
    let failed = events
        .iter()
        .filter(|e| e.kind == EventKind::SeatLaunchFailed && e.task.as_deref() == Some(task))
        .filter(|e| {
            e.payload["reason"]
                .as_str()
                .is_some_and(|r| r.starts_with(UNCONFIRMED_PREFIX))
        })
        .count();
    u32::try_from(failed).unwrap_or(u32::MAX)
}

fn stop_kills(events: &[FleetEvent], task: &str) -> u32 {
    let kills = events
        .iter()
        .filter(|e| e.kind == EventKind::SeatClosed && e.task.as_deref() == Some(task))
        .filter(|e| e.payload["reason"].as_str() == Some(STOP_KILL))
        .count();
    u32::try_from(kills).unwrap_or(u32::MAX)
}

pub fn seat_facts(
    task: &str,
    session: Uuid,
    events: &[FleetEvent],
    seen: &Observed,
) -> Option<SeatFacts> {
    let mine: Vec<&FleetEvent> = events.iter().filter(|e| about(e, task, session)).collect();
    let requested = mine
        .iter()
        .rev()
        .find(|e| e.kind == EventKind::SeatLaunchRequested)?;
    let confirmed = seen.confirmed_by_request
        || mine
            .iter()
            .any(|e| e.kind == EventKind::SeatLaunchConfirmed);
    let closed = mine.iter().any(|e| {
        e.kind == EventKind::SeatClosed && e.payload["reason"].as_str() != Some(STOP_KILL)
    });
    let woken_at = mine
        .iter()
        .rev()
        .find(|e| e.kind == EventKind::SeatWoken)
        .map(|e| e.occurred_at);
    let relaunched = relaunches(events, task);
    Some(SeatFacts {
        task: task.to_string(),
        session,
        launch_id: launch_id(requested),
        requested_at: requested.occurred_at,
        confirmed,
        closed,
        process_gone: seen.process_gone,
        last_seen: seen.last_seen,
        unread_mail: seen.unread_mail,
        woken_at,
        relaunches: relaunched,
        route_suspect: false,
        stopping: seen.stopping,
        stop_kills: stop_kills(events, task),
    })
}
