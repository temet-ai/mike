use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::{STEPS, SeatCloser, accepted_close_event, close_accepted_seats, should_close};
use crate::launch::fleet::events::{EventKind, FleetEvent};
use crate::launch::fleet::fold::fold;
use crate::launch::fleet::staff_limits::live_worker_seats;
use crate::launch::fleet::status::{FleetStatus, SeatStatus, TaskStatus};

struct FakeCloser {
    asked: Mutex<Vec<(String, Uuid)>>,
}

impl SeatCloser for FakeCloser {
    async fn close(&self, task: &str, session: Uuid) -> bool {
        self.asked
            .lock()
            .map(|mut asked| asked.push((task.to_string(), session)))
            .is_ok()
    }
}

fn session_of(n: u128) -> Uuid {
    Uuid::from_u128(0x100 + n)
}

fn kit_host_status() -> anyhow::Result<FleetStatus> {
    let mut status = crate::launch::fleet::render::tests_fixture::fixture()?;
    let template_task = status.tasks[0].clone();
    let template_seat = status.seats[0].clone();
    let task = |id: &str, state: &str, ready: bool, seat: Option<&str>| TaskStatus {
        id: id.to_string(),
        state: state.to_string(),
        ready,
        seat: seat.map(str::to_string),
        ..template_task.clone()
    };
    let seat = |name: &str, n: u128| SeatStatus {
        seat: name.to_string(),
        session: Some(session_of(n)),
        closed: false,
        ..template_seat.clone()
    };
    status.tasks = vec![
        task("T1", "passed", true, Some("rust-T1")),
        task("T2", "passed", true, Some("rust-T2")),
        task("T3", "rework", true, Some("rust-T3")),
        task("T4", "queued", false, None),
        task("T5", "queued", false, None),
    ];
    status.seats = vec![seat("rust-T1", 1), seat("rust-T2", 2), seat("rust-T3", 3)];
    Ok(status)
}

fn recorded(at: DateTime<Utc>, task: &str, session: Uuid, kind: EventKind) -> FleetEvent {
    FleetEvent {
        id: 0,
        run_id: "run".into(),
        task: Some(task.to_string()),
        session_uuid: Some(session),
        kind,
        payload: serde_json::json!({}),
        occurred_at: at,
    }
}

#[tokio::test]
async fn an_accepted_tasks_open_seat_is_closed_and_no_longer_counts_against_the_seat_limit()
-> anyhow::Result<()> {
    let status = kit_host_status()?;
    assert_eq!(live_worker_seats(&status), 3);
    let closer = FakeCloser {
        asked: Mutex::new(Vec::new()),
    };
    let closed = close_accepted_seats(&closer, &status).await;
    assert_eq!(closed, vec!["T1".to_string(), "T2".to_string()]);
    let asked = closer
        .asked
        .lock()
        .map_err(|_| anyhow::anyhow!("closer lock poisoned"))?
        .clone();
    assert_eq!(
        asked,
        vec![
            ("T1".to_string(), session_of(1)),
            ("T2".to_string(), session_of(2))
        ],
        "a task sent back for rework keeps its seat"
    );
    let start = Utc::now();
    let mut events = Vec::new();
    for (n, task) in [(1, "T1"), (2, "T2"), (3, "T3")] {
        events.push(recorded(
            start,
            task,
            session_of(n),
            EventKind::SeatLaunchConfirmed,
        ));
    }
    for (task, session) in &asked {
        let new = accepted_close_event("run", task, *session);
        let mut event = recorded(
            start + Duration::seconds(5),
            task,
            *session,
            new.payload.kind()?,
        );
        event.payload = serde_json::to_value(&new.payload)?;
        events.push(event);
    }
    let folded = fold(&events, DateTime::<Utc>::MAX_UTC);
    let mut after = status.clone();
    for task in &mut after.tasks {
        if folded.seat_of(&task.id).is_none() {
            task.seat = None;
        }
    }
    assert_eq!(folded.seat_of("T3"), Some(session_of(3)));
    assert_eq!(live_worker_seats(&after), 1);
    Ok(())
}

#[test]
fn the_tick_runs_staff_before_it_tells_anyone_and_closes_last() {
    let names: Vec<&str> = STEPS.iter().map(|step| step.name).collect();
    assert_eq!(
        names,
        vec![
            "staff",
            "supervise",
            "settle_checks",
            "shared_tree",
            "judge",
            "close_accepted",
            "problems",
            "introduce",
            "wake",
            "effort",
            "tell",
            "escalate",
            "close"
        ]
    );
}

#[test]
fn the_loop_closes_a_running_fleet_once_every_task_is_accepted_and_no_control_seat_is_live() {
    assert!(should_close("running", true, false));
    assert!(!should_close("running", false, false));
    assert!(
        !should_close("running", true, true),
        "a live coordinator or acceptance seat closes the fleet itself"
    );
    assert!(!should_close("stopping", true, false));
    assert!(!should_close("done", true, false));
}

#[test]
fn steps_include_problems_after_shared_tree() {
    let names: Vec<&str> = STEPS.iter().map(|step| step.name).collect();
    let shared = names.iter().position(|name| *name == "shared_tree");
    let problems = names.iter().position(|name| *name == "problems");
    assert!(shared.is_some() && problems.is_some() && shared < problems);
}

#[test]
fn every_step_has_a_distinct_name() {
    let mut names: Vec<&str> = STEPS.iter().map(|step| step.name).collect();
    names.sort_unstable();
    names.dedup();
    assert_eq!(names.len(), STEPS.len());
}
