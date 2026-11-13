use std::collections::BTreeSet;

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::*;
use crate::launch::fleet::status::SeatStatus;

fn at(text: &str) -> DateTime<Utc> {
    text.parse().unwrap_or_default()
}

fn seat(name: &str, tokens: Option<i64>, closed: bool) -> SeatStatus {
    SeatStatus {
        seat: name.into(),
        role: name.into(),
        model: "opus".into(),
        state: "working".into(),
        session: None,
        goal: None,
        context_tokens: tokens,
        requests: 9,
        last_request_at: Some(at("2026-10-08T09:00:00Z")),
        opened_at: Some(at("2026-10-08T07:00:00Z")),
        closed,
        first_request_cache: None,
    }
}

fn status(seats: Vec<SeatStatus>) -> FleetStatus {
    FleetStatus {
        fleet: "f".into(),
        slug: "s".into(),
        state: "running".into(),
        run_id: Some("fleet-s-1".into()),
        goal: String::new(),
        started_at: Some(at("2026-10-08T07:00:00Z")),
        finished_at: None,
        now: at("2026-10-08T09:08:00Z"),
        missing: vec![],
        criteria: vec![],
        tasks: vec![],
        seats,
        verdicts: vec![],
        blocked: vec![],
        needs_operator: vec![],
        accepted: false,
        obligations: vec![],
        every_seat_idle_seconds: None,
        tree_broken: None,
        problems: Vec::new(),
    }
}

#[test]
fn a_control_seat_past_the_line_is_reported_to_the_coordinator_naming_seat_and_notes() {
    let coordinator = Uuid::from_u128(1);
    let seats = vec![
        seat("acceptance", Some(689_979), false),
        seat("observer", Some(297_246), false),
        seat("coordinator", Some(420_738), false),
        seat("backend-T5", Some(900_000), false),
    ];
    let sent = past_the_line("fleet-id", coordinator, &status(seats), |_| false);
    assert_eq!(sent.len(), 1, "{sent:?}");
    assert_eq!(sent[0].to, coordinator);
    assert_eq!(sent[0].key, "context-handoff:acceptance");
    assert!(sent[0].body.contains("acceptance seat's context is 690k"));
    assert!(sent[0].body.contains("fleet:fleet-id"));
    assert!(sent[0].body.contains("acceptance-obligation"));
}

#[test]
fn a_seat_already_reported_or_closed_or_exactly_at_the_line_is_handled_once() {
    let coordinator = Uuid::from_u128(1);
    let told: BTreeSet<String> = ["context-handoff:acceptance".to_string()].into();
    let seats = vec![
        seat("acceptance", Some(650_000), false),
        seat("observer", Some(700_000), true),
        seat("reference", Some(REPLACE_AT), false),
        seat("coordinator", Some(REPLACE_AT - 1), false),
    ];
    let sent = past_the_line("fleet-id", coordinator, &status(seats), |k| {
        told.contains(k)
    });
    let keys: Vec<&str> = sent.iter().map(|n| n.key.as_str()).collect();
    assert_eq!(keys, vec!["context-handoff:reference"]);
}

#[test]
fn a_seat_with_no_requests_yet_is_not_reported() {
    let sent = past_the_line(
        "fleet-id",
        Uuid::from_u128(1),
        &status(vec![seat("acceptance", None, false)]),
        |_| false,
    );
    assert!(sent.is_empty());
}

#[test]
fn control_roles_and_numbered_successors_are_reported_without_matching_worker_prefixes() {
    let mut seats = Vec::new();
    for role in CONTROL_SEATS {
        let mut renamed = seat(&format!("replacement-{role}"), Some(REPLACE_AT), false);
        renamed.role = role.into();
        seats.push(renamed);
        let mut successor = seat(&format!("{role}-2"), Some(REPLACE_AT), false);
        successor.role = String::new();
        seats.push(successor);
    }
    for name in [
        "acceptance-other",
        "observer-",
        "coordinator-2-T3",
        "reference-T1",
    ] {
        seats.push(seat(name, Some(REPLACE_AT), false));
    }
    let sent = past_the_line("fleet-id", Uuid::from_u128(1), &status(seats), |_| false);
    assert_eq!(sent.len(), 8);
}

#[test]
fn each_actual_session_reusing_a_seat_gets_its_own_notice() {
    let mut first = seat("acceptance", Some(REPLACE_AT), false);
    first.session = Some(Uuid::from_u128(2));
    let first_notice = past_the_line(
        "fleet-id",
        Uuid::from_u128(1),
        &status(vec![first.clone()]),
        |_| false,
    );
    let mut successor = first.clone();
    successor.session = Some(Uuid::from_u128(3));
    let sent = past_the_line(
        "fleet-id",
        Uuid::from_u128(1),
        &status(vec![first, successor]),
        |key| key == first_notice[0].key || key == "context-handoff:acceptance",
    );
    assert_eq!(sent.len(), 1);
    assert_eq!(
        sent[0].key,
        "context-handoff:acceptance:00000000-0000-0000-0000-000000000003"
    );
    assert!(sent[0].body.contains("once per recorded session"));
}

#[test]
fn silence_and_seat_state_do_not_imply_a_context_problem_or_a_hang() {
    let mut waiting = seat("coordinator-2", Some(REPLACE_AT - 1), false);
    waiting.state = "waiting".into();
    waiting.last_request_at = Some(at("2026-10-01T09:00:00Z"));
    let mut no_context = waiting.clone();
    no_context.context_tokens = None;
    let sent = past_the_line(
        "fleet-id",
        Uuid::from_u128(1),
        &status(vec![waiting, no_context]),
        |_| false,
    );
    assert!(sent.is_empty());
}
