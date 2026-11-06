use chrono::Utc;
use serde_json::json;

use super::{INTERRUPTED, live, unfinished};
use crate::launch::fleet::events::{EventKind, FleetEvent};

fn event(id: i64, kind: EventKind, receipt: i64) -> FleetEvent {
    FleetEvent {
        id,
        run_id: "fleet-x".into(),
        task: None,
        session_uuid: None,
        kind,
        payload: json!({"receipt_id": receipt}),
        occurred_at: Utc::now(),
    }
}

#[test]
fn a_queued_receipt_with_no_finish_and_no_runner_is_unfinished() {
    let events = vec![
        event(1, EventKind::CheckQueued, 9_000_001),
        event(2, EventKind::CheckQueued, 9_000_002),
        event(3, EventKind::CheckFinished, 9_000_002),
    ];
    assert_eq!(unfinished(&events), vec![9_000_001]);
}

#[test]
fn a_receipt_whose_runner_is_alive_is_not_interrupted() {
    live().insert(9_000_011);
    let events = vec![event(1, EventKind::CheckQueued, 9_000_011)];
    assert!(unfinished(&events).is_empty());
    live().remove(&9_000_011);
    assert_eq!(unfinished(&events), vec![9_000_011]);
}

#[test]
fn the_interrupted_state_is_named_interrupted() {
    assert_eq!(INTERRUPTED, "interrupted");
}
