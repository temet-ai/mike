use std::path::PathBuf;

use anyhow::{Context, Result};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

use super::events::{EventKind, FleetEvent};
use super::fold::{Folded, fold};

fn fixture(run: &str, file: &str) -> Result<Vec<Value>> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures/fleet-runs")
        .join(run)
        .join(file);
    let text = std::fs::read_to_string(&path).with_context(|| path.display().to_string())?;
    Ok(serde_json::from_str(&text)?)
}

fn at(value: &Value, key: &str) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(value.get(key)?.as_str()?)
        .ok()
        .map(|t| t.with_timezone(&Utc))
}

fn instant(text: &str) -> Result<DateTime<Utc>> {
    Ok(DateTime::parse_from_rfc3339(text)?.with_timezone(&Utc))
}

fn session(value: &Value) -> Option<Uuid> {
    value.get("session_uuid")?.as_str()?.parse().ok()
}

fn task_of_seat(seat: &str) -> Option<String> {
    let last = seat.rsplit('-').next()?;
    let digits = last.strip_prefix('T')?;
    (!digits.is_empty() && digits.chars().all(|c| c.is_ascii_digit())).then(|| last.to_string())
}

fn event(
    kind: EventKind,
    task: Option<String>,
    session: Option<Uuid>,
    payload: Value,
    occurred_at: DateTime<Utc>,
) -> FleetEvent {
    FleetEvent {
        id: 0,
        run_id: "replay".into(),
        task,
        session_uuid: session,
        kind,
        payload,
        occurred_at,
    }
}

fn launch_events(rows: &[Value]) -> Vec<FleetEvent> {
    let mut out = Vec::new();
    for row in rows {
        let seat = row.get("seat").and_then(Value::as_str).unwrap_or_default();
        let task = task_of_seat(seat);
        let who = session(row);
        if row.get("result").and_then(Value::as_str) == Some("failed") {
            if let Some(created) = at(row, "created_at") {
                out.push(event(
                    EventKind::SeatLaunchFailed,
                    task,
                    who,
                    json!({"launch_id": 0, "reason": row.get("reason")}),
                    created,
                ));
            }
            continue;
        }
        if let Some(opened) = at(row, "opened_at") {
            out.push(event(
                EventKind::SeatLaunchConfirmed,
                task.clone(),
                who,
                json!({"launch_id": 0}),
                opened,
            ));
        }
        if let Some(closed) = at(row, "closed_at") {
            out.push(event(
                EventKind::SeatClosed,
                task,
                who,
                json!({"reason": "closed"}),
                closed,
            ));
        }
    }
    out
}

fn note_events(notes: &[Value]) -> Vec<FleetEvent> {
    let mut out = Vec::new();
    for note in notes {
        let data = note.get("data").cloned().unwrap_or_default();
        let task = data.get("task").and_then(Value::as_str).map(str::to_string);
        let (Some(created), kind) = (
            at(note, "created_at"),
            note.get("kind").and_then(Value::as_str),
        ) else {
            continue;
        };
        if kind == Some("report") {
            out.push(event(
                EventKind::ReportFiled,
                task.clone(),
                session(note),
                json!({}),
                created,
            ));
            if let Some(sha) = data.get("commit").and_then(Value::as_str) {
                out.push(event(
                    EventKind::CommitLanded,
                    task,
                    session(note),
                    json!({ "sha": sha }),
                    created,
                ));
            }
        }
    }
    out
}

fn replay(run: &str) -> Result<Vec<FleetEvent>> {
    let mut events = launch_events(&fixture(run, "launches.json")?);
    events.extend(note_events(&fixture(run, "notes.json")?));
    Ok(events)
}

fn state_at(events: &[FleetEvent], time: &str) -> Result<Folded> {
    Ok(fold(events, instant(time)?))
}

const WRITING_ROOM: &str = "fleet-writing-room-0656d7b7";
const CMS_UX: &str = "fleet-cms-ux-8d1a15dc";

#[test]
fn a_task_whose_launches_failed_has_no_seat_until_a_launch_is_confirmed() -> Result<()> {
    let events = replay(WRITING_ROOM)?;
    assert_eq!(
        state_at(&events, "2026-10-21T10:58:00Z")?.seat_of("T2"),
        None
    );
    let seated = state_at(&events, "2026-10-21T11:30:00Z")?.seat_of("T2");
    assert!(seated.is_some_and(|id| id.to_string().starts_with("0eeff442")));
    Ok(())
}

#[test]
fn a_task_never_shows_its_failed_launch_as_its_seat_after_the_run() -> Result<()> {
    let events = replay(WRITING_ROOM)?;
    let late = state_at(&events, "2026-10-21T18:00:00Z")?;
    assert_eq!(late.seat_of("T2"), None);
    Ok(())
}

#[test]
fn cms_ux_t6_and_t11_have_a_seat_at_every_commit_they_made() -> Result<()> {
    let events = replay(CMS_UX)?;
    let commits: Vec<&FleetEvent> = events
        .iter()
        .filter(|e| e.kind == EventKind::CommitLanded)
        .filter(|e| matches!(e.task.as_deref(), Some("T6" | "T11")))
        .collect();
    assert!(
        commits.len() >= 7,
        "expected the T6 and T11 commits, found {}",
        commits.len()
    );
    for commit in commits {
        let task = commit.task.as_deref().unwrap_or_default();
        let folded = fold(&events, commit.occurred_at);
        assert_eq!(
            folded.seat_of(task),
            commit.session_uuid,
            "{task} committed at {} without that session as its seat",
            commit.occurred_at
        );
    }
    Ok(())
}

#[test]
fn writing_room_is_running_while_a_seat_is_live_and_done_once_the_last_seat_closed() -> Result<()> {
    let events = replay(WRITING_ROOM)?;
    let midway = state_at(&events, "2026-10-21T12:00:00Z")?;
    assert!(midway.any_seat_live());
    assert_eq!(midway.fleet_state("running", true), "running");
    let end = state_at(&events, "2026-10-21T18:00:00Z")?;
    assert!(!end.any_seat_live());
    assert_eq!(end.fleet_state("running", true), "done");
    assert_eq!(end.fleet_state("running", false), "running");
    Ok(())
}

#[test]
fn a_report_waits_for_a_verdict_until_one_is_recorded() -> Result<()> {
    let events = replay(WRITING_ROOM)?;
    let waiting = state_at(&events, "2026-10-21T10:52:50Z")?;
    assert!(waiting.awaiting_verdict("T1"));
    let mut judged = events.clone();
    judged.push(event(
        EventKind::VerdictRecorded,
        Some("T1".into()),
        None,
        json!({}),
        instant("2026-10-21T10:53:00Z")?,
    ));
    assert!(!fold(&judged, instant("2026-10-21T10:54:00Z")?).awaiting_verdict("T1"));
    Ok(())
}

#[test]
fn a_stop_with_a_live_seat_is_stopping_and_without_one_is_done() -> Result<()> {
    let mut events = replay(WRITING_ROOM)?;
    events.push(event(
        EventKind::StopRequested,
        None,
        None,
        json!({"by": "operator"}),
        instant("2026-10-21T12:00:00Z")?,
    ));
    let live = state_at(&events, "2026-10-21T12:10:00Z")?;
    assert_eq!(live.fleet_state("running", false), "stopping");
    let closed = state_at(&events, "2026-10-21T18:00:00Z")?;
    assert_eq!(closed.fleet_state("stopping", false), "done");
    Ok(())
}

#[test]
fn a_producers_exports_commit_makes_it_ready_once_its_check_passes() -> Result<()> {
    let mut events = replay(CMS_UX)?;
    let commit = events
        .iter()
        .find(|e| e.kind == EventKind::CommitLanded && e.task.as_deref() == Some("T6"))
        .cloned()
        .context("a T6 commit in the fixture")?;
    let sha = commit.payload["sha"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(!fold(&events, commit.occurred_at).exports_passed("T6"));
    events.push(event(
        EventKind::CheckFinished,
        None,
        None,
        json!({"receipt_id": 1, "sha": sha, "passed": false}),
        commit.occurred_at,
    ));
    assert!(!fold(&events, DateTime::<Utc>::MAX_UTC).exports_passed("T6"));
    events.push(event(
        EventKind::CheckFinished,
        None,
        None,
        json!({"receipt_id": 2, "sha": sha, "passed": true}),
        commit.occurred_at,
    ));
    assert!(fold(&events, DateTime::<Utc>::MAX_UTC).exports_passed("T6"));
    Ok(())
}

fn refused(task: &str, at: &str, reason: &str) -> Result<FleetEvent> {
    Ok(event(
        EventKind::SeatLaunchFailed,
        Some(task.into()),
        Some(Uuid::new_v4()),
        json!({"launch_id": 0, "reason": reason}),
        instant(at)?,
    ))
}

#[test]
fn a_task_whose_launch_was_refused_three_times_is_not_started_and_says_why() -> Result<()> {
    let events = vec![
        refused("T2", "2026-10-21T10:00:00Z", "no terminal surface")?,
        refused("T2", "2026-10-21T10:01:00Z", "no terminal surface")?,
    ];
    assert_eq!(
        fold(&events, DateTime::<Utc>::MAX_UTC).not_started("T2"),
        None
    );
    let mut three = events;
    three.push(refused(
        "T2",
        "2026-10-21T10:02:00Z",
        "no terminal surface",
    )?);
    let state = fold(&three, DateTime::<Utc>::MAX_UTC).not_started("T2");
    assert!(
        state.is_some_and(|s| s.starts_with("not started after 3 attempts: no terminal surface")),
    );
    Ok(())
}

#[test]
fn a_confirmed_launch_between_refusals_restarts_the_count() -> Result<()> {
    let mut events = vec![
        refused("T2", "2026-10-21T10:00:00Z", "x")?,
        refused("T2", "2026-10-21T10:01:00Z", "x")?,
    ];
    events.push(event(
        EventKind::SeatLaunchConfirmed,
        Some("T2".into()),
        Some(Uuid::new_v4()),
        json!({"launch_id": 1}),
        instant("2026-10-21T10:02:00Z")?,
    ));
    events.push(refused("T2", "2026-10-21T10:03:00Z", "x")?);
    assert_eq!(
        fold(&events, DateTime::<Utc>::MAX_UTC).not_started("T2"),
        None
    );
    Ok(())
}

#[test]
fn a_task_keeps_its_last_seat_after_that_seat_closes_but_has_no_live_seat() -> Result<()> {
    let events = replay(WRITING_ROOM)?;
    let closed = state_at(&events, "2026-10-21T12:00:00Z")?;
    assert_eq!(closed.seat_of("T2"), None);
    assert!(
        closed
            .last_seat("T2")
            .is_some_and(|id| id.to_string().starts_with("0eeff442"))
    );
    let before = state_at(&events, "2026-10-21T10:58:00Z")?;
    assert_eq!(before.last_seat("T2"), None);
    Ok(())
}

#[test]
fn a_task_is_done_when_every_criterion_it_serves_has_a_passing_verdict_and_no_report_waits()
-> Result<()> {
    let def = crate::launch::fleet::definition::FleetDefinition::from_value(&json!({
        "tasks": {
            "T1": {"name": "a", "discipline": "backend", "criteria": ["C1", "C2"]},
            "T2": {"name": "b", "discipline": "backend", "criteria": ["C1"]}
        }
    }))
    .map_err(anyhow::Error::msg)?;
    let verdict = |task: &str, criterion: &str, passed: bool, at: &str| -> Result<FleetEvent> {
        Ok(event(
            EventKind::VerdictRecorded,
            Some(task.into()),
            None,
            json!({"note_uuid": Uuid::nil(), "criterion": criterion, "passed": passed}),
            instant(at)?,
        ))
    };
    let mut events = vec![
        verdict("T1", "C1", true, "2026-10-21T10:00:00Z")?,
        verdict("T2", "C1", true, "2026-10-21T10:00:00Z")?,
    ];
    let done = fold(&events, DateTime::<Utc>::MAX_UTC).done_tasks(&def);
    assert_eq!(done.into_iter().collect::<Vec<_>>(), vec!["T2".to_string()]);
    events.push(verdict("T1", "C2", true, "2026-10-21T10:01:00Z")?);
    let done = fold(&events, DateTime::<Utc>::MAX_UTC).done_tasks(&def);
    assert_eq!(done.len(), 2);
    events.push(verdict("T1", "C2", false, "2026-10-21T10:02:00Z")?);
    let done = fold(&events, DateTime::<Utc>::MAX_UTC).done_tasks(&def);
    assert_eq!(done.into_iter().collect::<Vec<_>>(), vec!["T2".to_string()]);
    Ok(())
}
