use std::collections::BTreeSet;

use chrono::Utc;

use super::{launched_seats, never_started};
use crate::launch::fleet::plan::worker_seat_name;
use crate::launch::fleet::test_fixtures::{complete, parsed};
use crate::store::persistence::fleet::FleetSeatRow;

fn set(items: &[&str]) -> BTreeSet<String> {
    items.iter().map(|item| (*item).to_string()).collect()
}

#[test]
fn tasks_without_after_and_without_a_seat_still_need_one_after_the_templates_close() {
    let def = parsed(&complete());
    let unstarted = set(&["P1", "P2", "P10"]);
    assert_eq!(
        never_started(&def, &set(&[]), &unstarted, &set(&[])),
        vec!["P1".to_string(), "P2".to_string()]
    );
}

#[test]
fn a_task_whose_seat_was_launched_no_longer_holds_the_fleet_open() {
    let def = parsed(&complete());
    let unstarted = set(&["P1", "P2", "P10"]);
    let launched = set(&[
        &worker_seat_name("db-dev", "P1"),
        &worker_seat_name("frontend-developer", "P2"),
    ]);
    assert!(never_started(&def, &set(&[]), &unstarted, &launched).is_empty());
}

#[test]
fn a_task_waiting_on_an_unreleased_task_does_not_hold_the_fleet_open() {
    let def = parsed(&complete());
    assert!(never_started(&def, &set(&[]), &set(&["P10"]), &set(&[])).is_empty());
    assert_eq!(
        never_started(&def, &set(&["P2"]), &set(&["P10"]), &set(&[])),
        vec!["P10".to_string()]
    );
}

fn seat_row(seat: &str, opened: bool, requests: i64) -> FleetSeatRow {
    FleetSeatRow {
        seat: Some(seat.to_string()),
        role: None,
        agent: None,
        model: None,
        session_uuid: None,
        surface_ref: None,
        opened_at: opened.then(Utc::now),
        closed_at: Some(Utc::now()),
        requests,
        last_request_at: None,
        context_tokens: None,
        last_status: None,
        forked: false,
        first_cache_read: None,
        first_cache_written: None,
    }
}

#[test]
fn a_launch_that_never_opened_does_not_count_as_a_started_seat() {
    let rows = [
        seat_row("db-dev-P1", false, 0),
        seat_row("frontend-developer-P2", true, 0),
        seat_row("frontend-developer-P10", false, 3),
    ];
    assert_eq!(
        launched_seats(&rows),
        set(&["frontend-developer-P2", "frontend-developer-P10"])
    );
}

#[test]
fn a_fleet_with_no_queued_task_has_nothing_waiting_to_start() {
    let def = parsed(&complete());
    assert!(never_started(&def, &set(&["P1", "P2"]), &set(&[]), &set(&[])).is_empty());
}
