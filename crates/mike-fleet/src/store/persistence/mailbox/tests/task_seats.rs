use chrono::DateTime;
use serde_json::json;
use uuid::Uuid;

use super::super::fleet_target::{
    FleetTarget, TargetError, seat_of_owner, seat_of_task, seat_sessions,
};
use crate::launch::fleet::FleetDefinition;
use crate::launch::fleet::definition::{recorded_task_of, with_task_seats};
use crate::launch::fleet::events::{EventKind, FleetEvent};

fn definition() -> anyhow::Result<FleetDefinition> {
    FleetDefinition::from_value(&json!({
        "coordinator": {"session": Uuid::from_u128(1).to_string()},
        "tasks": {
            "T1": {"files": ["crates/mike/src/owned/"]},
            "T2": {"files": ["docs/T2.md"]},
            "T3": {"files": ["docs/T3.md"]}
        }
    }))
    .map_err(anyhow::Error::msg)
}

fn event(id: i64, task: &str, session: Uuid, kind: EventKind) -> FleetEvent {
    FleetEvent {
        id,
        run_id: "fleet-task-seats".into(),
        task: Some(task.into()),
        session_uuid: Some(session),
        kind,
        payload: json!({}),
        occurred_at: DateTime::from_timestamp(id * 10, 0).unwrap_or_default(),
    }
}

fn seat(session: Uuid, task: &str) -> FleetTarget {
    FleetTarget::Seat {
        session,
        task: Some(task.into()),
    }
}

#[test]
fn a_seat_known_only_from_its_launch_event_is_reachable_by_task_owner_and_fleet()
-> anyhow::Result<()> {
    let (first, successor, ended) = (
        Uuid::from_u128(11),
        Uuid::from_u128(12),
        Uuid::from_u128(13),
    );
    let events = [
        event(1, "T1", first, EventKind::SeatLaunchConfirmed),
        event(2, "T2", ended, EventKind::SeatLaunchConfirmed),
        event(3, "T1", first, EventKind::SeatClosed),
        event(4, "T2", ended, EventKind::SeatClosed),
        event(5, "T1", successor, EventKind::SeatLaunchConfirmed),
    ];
    let raw = definition()?;
    assert!(matches!(
        seat_of_task(&raw, "T1"),
        Err(TargetError::NoSeatYet { .. })
    ));
    let def = with_task_seats(raw, &events);
    assert_eq!(seat_of_task(&def, "T1")?, seat(successor, "T1"));
    assert_eq!(
        seat_of_owner(&def, "crates/mike/src/owned/x.rs")?,
        seat(successor, "T1")
    );
    assert_eq!(seat_of_task(&def, "T2")?, seat(ended, "T2"));
    assert!(matches!(
        seat_of_task(&def, "T3"),
        Err(TargetError::NoSeatYet { .. })
    ));
    assert!(seat_sessions(&def).contains(&successor));
    assert_eq!(recorded_task_of(&events, first).as_deref(), Some("T1"));
    Ok(())
}

#[test]
fn a_definition_session_stays_when_the_record_has_no_seat_for_the_task() -> anyhow::Result<()> {
    let held = Uuid::from_u128(21);
    let mut raw = definition()?;
    if let Some(task) = raw.tasks.get_mut("T3") {
        task.session = Some(held.to_string());
    }
    let def = with_task_seats(raw, &[]);
    assert_eq!(seat_of_task(&def, "T3")?, seat(held, "T3"));
    Ok(())
}

#[test]
fn a_task_with_no_seat_says_what_it_waits_on_and_to_ask_instead() -> anyhow::Result<()> {
    let mut def = definition()?;
    if let Some(task) = def.tasks.get_mut("T3") {
        task.after = vec!["T1".into(), "T2".into()];
    }
    let Err(waiting) = seat_of_task(&def, "T3") else {
        anyhow::bail!("T3 has no seat and must be refused");
    };
    let text = waiting.to_string();
    assert!(text.starts_with("task T3 has no seat yet: nothing has been started for it"));
    assert!(text.contains("starts after T1, T2"), "{text}");
    assert!(text.contains("the `ask` tool, to task:T3"), "{text}");
    let Err(slot) = seat_of_task(&def, "T2") else {
        anyhow::bail!("T2 has no seat and must be refused");
    };
    assert!(slot.to_string().contains("free seat slot"), "{slot}");
    Ok(())
}
