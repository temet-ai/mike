use serde_json::{Value, json};
use uuid::Uuid;

use super::definition::{FleetDefinition, merge};
use super::report::ReportRequest;
use super::report_input::fill_seat_defaults;
use super::seated::Seated;
use super::tests::{complete, parsed};

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

fn def() -> FleetDefinition {
    let mut value: Value = complete();
    merge(
        &mut value,
        &json!({"tasks": {
            "P1": {"seat": "db-dev-P1", "session": id(14).to_string()},
            "P2": {"seat": "frontend-developer-P2", "session": id(15).to_string()}
        }}),
    );
    parsed(&value)
}

fn request(session: u128) -> ReportRequest {
    ReportRequest {
        session: Some(id(session)),
        ..ReportRequest::default()
    }
}

#[test]
fn fill_seat_defaults_takes_the_task_and_slice_from_the_seat() {
    let held = Seated::default().worker("P1", id(14), "db-dev-P1");
    let mut req = request(14);
    assert_eq!(fill_seat_defaults(&mut req, &def(), &held), Ok(()));
    assert_eq!(req.task, "P1");
    assert_eq!(req.slice, "task P1");
}

#[test]
fn fill_seat_defaults_keeps_a_named_task_and_slice() {
    let mut req = request(14);
    req.task = "P2".into();
    req.slice = "reads".into();
    assert_eq!(
        fill_seat_defaults(&mut req, &def(), &Seated::default()),
        Ok(())
    );
    assert_eq!((req.task.as_str(), req.slice.as_str()), ("P2", "reads"));
}

#[test]
fn fill_seat_defaults_refuses_a_seat_with_two_tasks_listing_them() {
    let mut value: Value = complete();
    merge(
        &mut value,
        &json!({"tasks": {
            "P1": {"seat": "s", "session": id(14).to_string()},
            "P2": {"seat": "s", "session": id(14).to_string()}
        }}),
    );
    let mut req = request(14);
    let why = fill_seat_defaults(&mut req, &parsed(&value), &Seated::default())
        .err()
        .unwrap_or_default();
    assert!(why.contains("holds P1, P2"), "{why}");
}

#[test]
fn fill_seat_defaults_refuses_a_seat_with_no_task_listing_the_fleet_tasks() {
    let mut req = request(99);
    let why = fill_seat_defaults(&mut req, &def(), &Seated::default())
        .err()
        .unwrap_or_default();
    assert!(why.contains("holds none") && why.contains("P1"), "{why}");
}

#[test]
fn unestablished_accepts_strings_and_objects() {
    let req: ReportRequest = serde_json::from_value(json!({
        "unestablished": [
            "plain",
            {"criterion": "C2", "text": "no live run"},
            {"summary": "only summary"},
            {"other": 1}
        ]
    }))
    .unwrap_or_default();
    assert_eq!(
        req.unestablished,
        Some(vec![
            "plain".to_string(),
            "C2: no live run".to_string(),
            "only summary".to_string(),
            "{\"other\":1}".to_string()
        ])
    );
    let one: ReportRequest =
        serde_json::from_value(json!({"unestablished": {"text": "x"}})).unwrap_or_default();
    assert_eq!(one.unestablished, Some(vec!["x".to_string()]));
}
