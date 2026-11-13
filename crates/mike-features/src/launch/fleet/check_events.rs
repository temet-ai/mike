use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::sync::OnceLock;

use dashmap::DashSet;

use crate::launch::fleet::events::{
    CheckFinished, CheckQueued, EventKind, EventPayload, NewFleetEvent,
};
use crate::store::persistence::PersistHandle;

pub const INTERRUPTED: &str = "interrupted";

fn live() -> &'static DashSet<i64> {
    static LIVE: OnceLock<DashSet<i64>> = OnceLock::new();
    LIVE.get_or_init(DashSet::new)
}

pub struct Finish<'a> {
    pub receipt: i64,
    pub sha: &'a str,
    pub state: &'a str,
    pub exit: Option<i32>,
}

pub fn state_of(
    row: &crate::store::persistence::fleet::TreeCheckRow,
) -> &'static str {
    let cancelled = row
        .state_after
        .as_deref()
        .is_some_and(|state| state.starts_with("cancelled"));
    if cancelled { "cancelled" } else { "finished" }
}

async fn append(persist: &PersistHandle, run: &str, payload: EventPayload) {
    let event = NewFleetEvent {
        run_id: super::base_run(run).to_string(),
        task: None,
        session_uuid: None,
        payload,
    };
    if let Err(err) = persist.fleet().append_fleet_event(&event).await {
        tracing::warn!(error = %err, run, "fleet: could not record a check event");
    }
}

pub async fn queued(
    persist: &PersistHandle,
    run: &str,
    receipt: i64,
    operation: &str,
) {
    live().insert(receipt);
    let payload = EventPayload::CheckQueued(CheckQueued {
        receipt_id: receipt,
        operation: operation.to_string(),
    });
    append(persist, run, payload).await;
}

pub async fn finished(
    persist: &PersistHandle,
    run: &str,
    finish: Finish<'_>,
) {
    live().remove(&finish.receipt);
    let payload = EventPayload::CheckFinished(CheckFinished {
        receipt_id: finish.receipt,
        sha: finish.sha.to_string(),
        passed: finish.exit == Some(0),
        state: finish.state.to_string(),
        exit: finish.exit,
    });
    append(persist, run, payload).await;
}

pub fn unfinished(
    events: &[crate::launch::fleet::events::FleetEvent],
) -> Vec<i64> {
    let receipt = |event: &crate::launch::fleet::events::FleetEvent| {
        event
            .payload
            .get("receipt_id")
            .and_then(serde_json::Value::as_i64)
    };
    let done: std::collections::HashSet<i64> = events
        .iter()
        .filter(|event| event.kind == EventKind::CheckFinished)
        .filter_map(receipt)
        .collect();
    events
        .iter()
        .filter(|event| event.kind == EventKind::CheckQueued)
        .filter_map(receipt)
        .filter(|id| !done.contains(id) && !live().contains(id))
        .collect()
}

pub async fn settle_interrupted(persist: &PersistHandle, run: &str) {
    let run = super::base_run(run);
    let Ok(events) = persist.fleet().fleet_events_since(run, 0).await else {
        return;
    };
    for receipt in unfinished(&events) {
        let finish = Finish {
            receipt,
            sha: "",
            state: INTERRUPTED,
            exit: None,
        };
        finished(persist, run, finish).await;
    }
}

#[cfg(test)]
#[path = "check_events_tests.rs"]
pub mod tests;
