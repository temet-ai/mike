use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use super::definition::FleetDefinition;
use super::events::{EventKind, FleetEvent};
use super::plan::ready_tasks;
use super::staff::passed_tasks;
use super::status::{FleetHeader, FleetStatus, NoteFacts, build, build_from_record};

const FLEET: &str = "00000000-0000-0000-0000-000000000001";

fn at(seconds: i64) -> DateTime<Utc> {
    DateTime::from_timestamp(seconds, 0).unwrap_or_default()
}

fn definition() -> anyhow::Result<FleetDefinition> {
    Ok(serde_json::from_value(json!({
        "criteria": {"C1": {"text": "setting persists"}, "C2": {"text": "setting is usable"}},
        "tasks": {
            "T1": {"name": "setting", "criteria": ["C1"], "discipline": "rust"},
            "T2": {"name": "settings journey", "criteria": ["C2"], "discipline": "frontend", "after": ["T1"]}
        }
    }))?)
}

fn decision(time: i64, data: Value) -> NoteFacts {
    NoteFacts {
        note_uuid: Uuid::from_u128(time.unsigned_abs().into()),
        session_uuid: Uuid::from_u128(1000),
        role: Some("acceptance".into()),
        seat: Some("acceptance".into()),
        kind: "decision".into(),
        title: "Observed outcome".into(),
        body: String::new(),
        status: None,
        tags: vec![],
        data,
        created_at: at(time),
        updated_at: at(time),
    }
}

fn obligation(time: i64, outcome: &str) -> NoteFacts {
    let mut note = decision(
        time,
        json!({
            "obligation_id": "setting-journey", "task": "T1", "criteria": ["C1"],
            "outcome": outcome, "observation": "Changed the setting, reopened it, and observed the persisted value."
        }),
    );
    note.kind = "task".into();
    note.tags = vec!["acceptance-obligation".into(), format!("fleet:{FLEET}")];
    note.body = "Full scenario and observed effects".into();
    note
}

fn final_pass(time: i64) -> NoteFacts {
    decision(
        time,
        json!({"verdict": "pass", "tasks": ["T1", "T2"], "criteria": ["C1", "C2"], "scope": "final"}),
    )
}

fn header() -> FleetHeader {
    FleetHeader {
        fleet: FLEET.into(),
        slug: "acceptance".into(),
        state: "done".into(),
        run_id: Some("run".into()),
        started_at: Some(at(0)),
        finished_at: Some(at(90)),
        now: at(100),
        missing: vec![],
    }
}

fn status(def: &FleetDefinition, notes: &[NoteFacts]) -> FleetStatus {
    build(header(), def, &[], notes)
}

fn judged(time: i64, task: &str, criterion: &str, passed: bool, needed: &str) -> FleetEvent {
    FleetEvent {
        id: time,
        run_id: "run".into(),
        task: Some(task.into()),
        session_uuid: None,
        kind: EventKind::VerdictRecorded,
        payload: json!({
            "note_uuid": Uuid::from_u128(time.unsigned_abs().into()),
            "criterion": criterion,
            "passed": passed,
            "needed": needed
        }),
        occurred_at: at(time),
    }
}

#[test]
fn the_judges_recorded_verdicts_accept_or_rework_a_task_when_no_acceptance_seat_exists()
-> anyhow::Result<()> {
    let def = definition()?;
    let events = [
        judged(1, "T1", "C1", true, ""),
        judged(2, "T2", "C2", false, "a journey test for the saved value"),
    ];
    let found = build_from_record(header(), &def, &[], &[], &events);
    assert_eq!(found.tasks[0].state, "passed");
    assert!(found.tasks[0].ready);
    assert_eq!((found.tasks[0].passes, found.tasks[0].fails), (1, 0));
    assert_eq!(found.tasks[1].state, "rework");
    assert_eq!((found.tasks[1].passes, found.tasks[1].fails), (0, 1));
    assert!(
        found.tasks[1]
            .last
            .as_ref()
            .is_some_and(|last| last.title.contains("a journey test for the saved value")),
        "{:?}",
        found.tasks[1].last
    );
    assert_eq!((found.criteria[0].passed, found.criteria[1].passed), (1, 0));
    assert!(!found.accepted);
    let events = [
        events[0].clone(),
        events[1].clone(),
        judged(3, "T2", "C2", true, ""),
    ];
    let found = build_from_record(header(), &def, &[], &[], &events);
    assert_eq!(found.tasks[1].state, "passed");
    assert!(found.accepted);
    Ok(())
}

#[test]
fn an_acceptance_seats_verdict_event_carries_its_session_and_is_read_from_its_note_only()
-> anyhow::Result<()> {
    let def = definition()?;
    let mut event = judged(1, "T1", "C1", true, "");
    event.session_uuid = Some(Uuid::from_u128(1000));
    let found = build_from_record(header(), &def, &[], &[], &[event.clone()]);
    assert_ne!(found.tasks[0].state, "passed");
    assert_eq!(found.tasks[0].passes, 0);
    let notes = [final_pass(2)];
    let found = build_from_record(header(), &def, &[], &notes, &[event]);
    assert_eq!(found.tasks[0].state, "passed");
    assert_eq!(
        found.tasks[0].passes, 1,
        "the note counts once, the event not again"
    );
    Ok(())
}

#[test]
fn ready_releases_dependents_while_required_product_check_remains_blocked() -> anyhow::Result<()> {
    let def = definition()?;
    let notes = [
        obligation(1, "blocked"),
        decision(2, json!({"task": "T1", "readiness": "ready"})),
    ];
    let found = status(&def, &notes);
    assert!(found.tasks[0].ready);
    assert_eq!(found.tasks[0].state, "ready");
    assert_eq!(found.criteria[0].ready, 1);
    assert_eq!(found.criteria[0].passed, 0);
    assert!(!found.accepted);
    assert!(ready_tasks(&def, &passed_tasks(&found)).contains(&"T2".to_string()));
    Ok(())
}

#[test]
fn partial_or_unscoped_pass_cannot_accept_even_before_register_seeding() -> anyhow::Result<()> {
    let def = definition()?;
    for data in [
        json!({"task": "T1", "verdict": "pass"}),
        json!({"task": "T1", "verdict": "pass", "criteria": ["C1"], "slice": 1}),
        json!({"task": "T1", "verdict": "pass", "criteria": ["C1"], "scope": "partial"}),
        json!({"task": "T1", "verdict": "pass", "criteria": ["C1"], "scope": null}),
        json!({"task": "T1", "verdict": "pass", "criteria": ["C1"], "scope": ""}),
        json!({"task": "T1", "verdict": "pass", "criteria": ["C1", 1], "scope": "final"}),
    ] {
        let found = status(&def, &[decision(1, data)]);
        assert!(found.tasks[0].passed_criteria.is_empty());
        assert!(!found.tasks[0].ready);
    }
    Ok(())
}

#[test]
fn explicit_protocol_requires_final_scope_and_settled_obligations() -> anyhow::Result<()> {
    let def = definition()?;
    for data in [
        json!({"task": "T1", "verdict": "pass", "criteria": ["C1"]}),
        json!({"task": "T1", "verdict": "pass", "scope": "final"}),
    ] {
        let found = status(&def, &[obligation(1, "accepted"), decision(2, data)]);
        assert_eq!(found.criteria[0].passed, 0);
    }
    let found = status(&def, &[obligation(1, "pending"), final_pass(2)]);
    assert_eq!(found.criteria[0].passed, 0);
    assert!(!found.accepted);
    Ok(())
}

#[test]
fn worker_and_helper_decisions_never_grant_readiness_or_acceptance() -> anyhow::Result<()> {
    let mut def = definition()?;
    for seat in ["rust-T1", "acceptance-helper"] {
        let mut note = final_pass(1);
        note.seat = Some(seat.into());
        note.data["readiness"] = json!("ready");
        let found = status(&def, &[note]);
        assert_eq!(found.criteria[0].passed, 0);
        assert!(!found.tasks[0].ready);
        assert!(found.verdicts.is_empty());
    }
    def.tasks
        .get_mut("T1")
        .ok_or_else(|| anyhow::anyhow!("T1"))?
        .session = Some(Uuid::from_u128(1000).to_string());
    assert!(!status(&def, &[final_pass(1)]).accepted);
    Ok(())
}

#[test]
fn task_marked_done_and_run_closed_do_not_settle_acceptance() -> anyhow::Result<()> {
    let mut def = definition()?;
    def.tasks
        .get_mut("T1")
        .ok_or_else(|| anyhow::anyhow!("T1"))?
        .state = Some("done".into());
    let found = status(&def, &[obligation(1, "pending"), final_pass(2)]);
    assert_eq!(found.state, "done");
    assert_eq!(found.tasks[0].state, "queued");
    assert!(!found.accepted);
    assert!(!found.tasks[0].ready);
    Ok(())
}

#[test]
fn acceptance_successor_settles_original_obligation_without_losing_its_record() -> anyhow::Result<()>
{
    let def = definition()?;
    let first = obligation(1, "blocked");
    let mut successor = obligation(2, "accepted");
    successor.session_uuid = Uuid::from_u128(1001);
    successor.seat = Some("acceptance-2".into());
    successor.data["supersedes"] = json!(first.note_uuid);
    let found = status(&def, &[first, successor.clone(), final_pass(3)]);
    assert!(found.accepted);
    assert_eq!(found.obligations.len(), 1);
    assert_eq!(found.obligations[0].note_id, successor.note_uuid);
    assert_eq!(found.obligations[0].data, successor.data);
    assert_eq!(found.obligations[0].body, successor.body);
    Ok(())
}

#[test]
fn duplicate_id_or_orphan_supersession_remains_unresolved() -> anyhow::Result<()> {
    let def = definition()?;
    let first = obligation(1, "pending");
    let second = obligation(2, "accepted");
    let found = status(&def, &[first, second.clone(), final_pass(3)]);
    assert!(!found.accepted);
    assert!(found.obligations[0].issue.is_some());
    let mut orphan = second;
    orphan.data["supersedes"] = json!(Uuid::from_u128(999));
    assert!(!status(&def, &[orphan, final_pass(3)]).accepted);
    Ok(())
}

#[test]
fn invalidation_blocks_until_existing_obligation_is_accepted_again() -> anyhow::Result<()> {
    let def = definition()?;
    let mut check = obligation(1, "accepted");
    let readiness = decision(2, json!({"task": "T1", "readiness": "ready"}));
    assert!(status(&def, &[check.clone(), readiness.clone(), final_pass(3)]).accepted);
    check.updated_at = at(4);
    check.data["outcome"] = json!("invalidated");
    let reopened = status(&def, &[check.clone(), readiness, final_pass(3)]);
    assert!(!reopened.accepted);
    assert!(reopened.tasks[0].ready);
    check.data["outcome"] = json!("accepted");
    assert!(status(&def, &[check, final_pass(3)]).accepted);
    Ok(())
}

#[test]
fn product_failure_does_not_revoke_dependency_readiness() -> anyhow::Result<()> {
    let def = definition()?;
    let found = status(
        &def,
        &[
            obligation(1, "failed"),
            decision(2, json!({"task": "T1", "readiness": "ready"})),
        ],
    );
    assert_eq!(found.tasks[0].state, "rework");
    assert!(found.tasks[0].ready);
    assert!(!found.accepted);
    Ok(())
}

#[test]
fn readiness_revocation_and_malformed_readiness_fail_closed_without_erasing_acceptance()
-> anyhow::Result<()> {
    let def = definition()?;
    for value in [json!("blocked"), json!(true), json!("typo")] {
        let found = status(
            &def,
            &[
                obligation(1, "accepted"),
                final_pass(2),
                decision(3, json!({"task": "T1", "readiness": value})),
            ],
        );
        assert!(found.accepted);
        assert!(!found.tasks[0].ready);
    }
    Ok(())
}

#[path = "acceptance_tests_more.rs"]
pub mod more;

#[path = "ruling_status_tests.rs"]
pub mod ruling_tests;
