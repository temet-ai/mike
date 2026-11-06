#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    reason = "tests assert on fixtures they built themselves; a broken fixture must fail loudly"
)]
use chrono::{DateTime, Utc};
use serde_json::json;
use uuid::Uuid;

use super::check::asker;
use super::commit::ownership;
use super::events::{EventKind, FleetEvent};
use super::seated::Seated;
use super::tests::{complete, parsed};
use crate::store::persistence::fleet::UnsettledFleetLaunch;

const RUN: &str = "fleet-seated-1";

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(seconds, 0).unwrap_or_default()
}

fn event(id: i64, task: &str, session: Uuid, kind: EventKind, seconds: i64) -> FleetEvent {
    FleetEvent {
        id,
        run_id: RUN.to_string(),
        task: Some(task.to_string()),
        session_uuid: Some(session),
        kind,
        payload: json!({}),
        occurred_at: at(seconds),
    }
}

fn launch(session: Uuid, seat: &str) -> UnsettledFleetLaunch {
    UnsettledFleetLaunch {
        id: 1,
        session_uuid: Some(session),
        backend: "none".to_string(),
        seat: Some(seat.to_string()),
        surface_ref: None,
        pid: None,
        opened: true,
        result: "success".to_string(),
        opened_at: Some(at(1)),
        closed_at: None,
        close_process_exited: None,
    }
}

#[test]
fn a_confirmed_launch_seats_the_session_with_no_session_field_on_the_definition() {
    let def = parsed(&complete());
    let session = Uuid::from_u128(5);
    let events = [event(1, "P1", session, EventKind::SeatLaunchConfirmed, 10)];
    let seated = Seated::from_record(&def, &events, &[]);
    assert!(def.tasks["P1"].session.is_none());
    let owner = ownership(&def, &seated, session).unwrap();
    assert_eq!(
        (owner.task.as_str(), owner.seat.as_str()),
        ("P1", "db-dev-P1")
    );
    assert_eq!(
        owner.owned,
        vec![
            "crates/mike/src/store/persistence/improvement",
            "crates/mike/src/store/persistence/mod.rs"
        ]
    );
    let who = asker(&def, &seated, session).unwrap();
    assert_eq!(who.seat, "db-dev-P1");
    assert!(!who.everything);
    assert!(seated.holds(session, "P1") && !seated.holds(session, "P2"));
    assert!(ownership(&def, &seated, Uuid::from_u128(6)).is_none());
    assert!(asker(&def, &seated, Uuid::from_u128(6)).is_none());
}

#[test]
fn a_seat_the_record_has_closed_or_failed_is_not_seated_even_by_an_open_launch_row() {
    let def = parsed(&complete());
    let closed = Uuid::from_u128(5);
    let failed = Uuid::from_u128(6);
    let events = [
        event(1, "P1", closed, EventKind::SeatLaunchConfirmed, 10),
        event(2, "P1", closed, EventKind::SeatClosed, 20),
        event(3, "P2", failed, EventKind::SeatLaunchFailed, 30),
    ];
    let launches = [
        launch(closed, "db-dev-P1"),
        launch(failed, "frontend-developer-P2"),
    ];
    let seated = Seated::from_record(&def, &events, &launches);
    assert!(seated.seat(closed).is_none());
    assert!(seated.seat(failed).is_none());
}

#[test]
fn an_open_launch_row_seats_a_session_the_supervisor_has_not_confirmed_yet() {
    let def = parsed(&complete());
    let first = Uuid::from_u128(7);
    let successor = Uuid::from_u128(8);
    let reference = Uuid::from_u128(9);
    let launches = [
        launch(first, "db-dev-P1"),
        launch(successor, "frontend-developer-P10-54ffa2fb"),
        launch(reference, "reference"),
    ];
    let seated = Seated::from_record(&def, &[], &launches);
    assert_eq!(seated.task_of(first), Some("P1"));
    assert_eq!(seated.task_of(successor), Some("P10"));
    assert_eq!(seated.task_of(reference), None);
    assert!(asker(&def, &seated, reference).unwrap().everything);
    assert!(ownership(&def, &seated, reference).is_none());
}

#[test]
fn the_launch_row_names_the_seat_and_the_task_names_it_when_there_is_none() {
    let def = parsed(&complete());
    let named = Uuid::from_u128(7);
    let unnamed = Uuid::from_u128(8);
    let events = [
        event(1, "P1", named, EventKind::SeatLaunchConfirmed, 10),
        event(2, "P2", unnamed, EventKind::SeatLaunchConfirmed, 10),
    ];
    let launches = [launch(named, "db-dev-P1-2a9f")];
    let seated = Seated::from_record(&def, &events, &launches);
    assert_eq!(seated.seat(named).unwrap().name, "db-dev-P1-2a9f");
    assert_eq!(seated.seat(unnamed).unwrap().name, "frontend-developer-P2");
}
