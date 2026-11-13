use std::sync::Mutex;
use std::time::Duration;

use chrono::{DateTime, TimeZone, Utc};
use serde_json::json;
use uuid::Uuid;

use super::facts::{Observed, STOP_KILL, UNCONFIRMED_PREFIX, seat_facts};
use super::*;
use crate::launch::fleet::events::{EventKind, FleetEvent};

struct FakeClock(DateTime<Utc>);

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        self.0
    }
}

#[derive(Default)]
struct Recorder {
    applied: Mutex<Vec<Action>>,
    refuse_kill: bool,
}

#[async_trait::async_trait]
impl Effects for Recorder {
    async fn apply(&self, action: &Action) -> Result<(), String> {
        if self.refuse_kill && matches!(action, Action::KillSeat { .. }) {
            return Err("the surface would not close".to_string());
        }
        if let Ok(mut applied) = self.applied.lock() {
            applied.push(action.clone());
        }
        Ok(())
    }
}

fn at(seconds: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_800_000_000 + seconds, 0)
        .single()
        .unwrap_or_default()
}

fn seat() -> SeatFacts {
    SeatFacts {
        task: "T10".to_string(),
        session: Uuid::from_u128(10),
        launch_id: 7,
        requested_at: at(0),
        confirmed: false,
        closed: false,
        process_gone: false,
        last_seen: None,
        unread_mail: false,
        woken_at: None,
        relaunches: 0,
        route_suspect: false,
        stopping: false,
        stop_kills: 0,
    }
}

fn kinds(steps: &[Action]) -> Vec<&'static str> {
    steps
        .iter()
        .map(|step| match step {
            Action::RecordLaunchFailed { .. } => "record_launch_failed",
            Action::KillSeat { .. } => "kill_seat",
            Action::Relaunch { .. } => "relaunch",
            Action::RouteSuspect { .. } => "route_suspect",
            Action::StopKill { .. } => "stop_kill",
            Action::Wake { .. } => "wake",
            Action::RecordProcessGone { .. } => "record_process_gone",
            Action::Replace { .. } => "replace",
            Action::RecordUnkillable { .. } => "record_unkillable",
            Action::KillCheck { .. } => "kill_check",
            Action::RequeueCheck { .. } => "requeue_check",
            Action::FailCheck { .. } => "fail_check",
            Action::SlotSuspect { .. } => "slot_suspect",
            Action::NeedsYou { .. } => "needs_you",
        })
        .collect()
}

#[test]
fn a_launch_not_confirmed_in_90_seconds_is_killed_and_relaunched() {
    let early = decide_seat(&seat(), at(89));
    assert!(early.is_empty(), "{early:?}");
    let steps = decide_seat(&seat(), at(90));
    assert_eq!(
        kinds(&steps),
        ["record_launch_failed", "kill_seat", "relaunch"]
    );
    assert!(matches!(&steps[2], Action::Relaunch { attempt: 1, .. }));
}

#[test]
fn a_launch_that_fails_twice_marks_the_route_suspect_and_raises_needs_you() {
    let mut second = seat();
    second.relaunches = 1;
    let steps = decide_seat(&second, at(95));
    assert!(matches!(&steps[2], Action::Relaunch { attempt: 2, .. }));
    let mut exhausted = seat();
    exhausted.relaunches = RELAUNCHES;
    let steps = decide_seat(&exhausted, at(95));
    assert_eq!(
        kinds(&steps),
        ["record_launch_failed", "kill_seat", "route_suspect"]
    );
    let Action::RouteSuspect { words, .. } = &steps[2] else {
        panic!("the last step names the suspect route");
    };
    assert!(
        words.contains("T10") && words.contains("launch row 7"),
        "{words}"
    );
}

#[test]
fn a_confirmed_launch_is_never_relaunched() {
    let mut confirmed = seat();
    confirmed.confirmed = true;
    assert!(decide_seat(&confirmed, at(600)).is_empty());
}

#[test]
fn a_seat_silent_five_minutes_with_unread_mail_gets_one_wake() {
    let mut quiet = seat();
    quiet.confirmed = true;
    quiet.last_seen = Some(at(0));
    quiet.unread_mail = true;
    assert!(decide_seat(&quiet, at(299)).is_empty());
    assert_eq!(kinds(&decide_seat(&quiet, at(300))), ["wake"]);
    quiet.unread_mail = false;
    assert!(decide_seat(&quiet, at(900)).is_empty());
}

#[test]
fn a_woken_seat_that_stays_silent_is_replaced() {
    let mut quiet = seat();
    quiet.confirmed = true;
    quiet.last_seen = Some(at(0));
    quiet.unread_mail = true;
    quiet.woken_at = Some(at(300));
    assert!(decide_seat(&quiet, at(400)).is_empty());
    let steps = decide_seat(&quiet, at(300 + 120));
    assert_eq!(kinds(&steps), ["kill_seat", "replace"]);
    quiet.last_seen = Some(at(350));
    assert!(decide_seat(&quiet, at(500)).is_empty());
}

#[test]
fn a_seat_whose_process_is_gone_is_replaced_at_the_next_pass() {
    let mut gone = seat();
    gone.confirmed = true;
    gone.process_gone = true;
    let steps = decide_seat(&gone, at(1));
    assert_eq!(kinds(&steps), ["record_process_gone", "replace"]);
    gone.closed = true;
    assert!(decide_seat(&gone, at(2)).is_empty());
}

fn check(requeues: u32) -> CheckFacts {
    CheckFacts {
        receipt_id: 41,
        task: Some("T10".to_string()),
        started_at: at(0),
        timeout: Duration::from_secs(1800),
        requeues,
    }
}

#[test]
fn a_check_past_its_timeout_is_killed_and_requeued_once() {
    assert!(decide_check(&check(0), at(1799)).is_empty());
    let steps = decide_check(&check(0), at(1800));
    assert_eq!(kinds(&steps), ["kill_check", "requeue_check"]);
}

#[test]
fn a_check_that_times_out_again_fails_and_its_owner_is_told() {
    let steps = decide_check(&check(CHECK_REQUEUES), at(1800));
    assert_eq!(kinds(&steps), ["kill_check", "fail_check"]);
    assert!(matches!(&steps[1], Action::FailCheck { task: Some(t), .. } if t == "T10"));
}

fn slot(name: &str, dirty: bool, suspect: bool) -> SlotFacts {
    SlotFacts {
        slot: name.to_string(),
        dirty,
        suspect,
    }
}

#[test]
fn a_dirty_slot_is_marked_suspect_and_the_clean_one_is_kept() {
    let steps = decide_slots(&[slot("a", true, false), slot("b", false, false)]);
    assert_eq!(kinds(&steps), ["slot_suspect"]);
    assert!(matches!(&steps[0], Action::SlotSuspect { slot } if slot == "a"));
    assert!(decide_slots(&[slot("a", true, true), slot("b", false, false)]).is_empty());
}

#[test]
fn when_every_slot_is_suspect_the_operator_is_asked() {
    let steps = decide_slots(&[slot("a", true, false), slot("b", false, true)]);
    assert_eq!(kinds(&steps), ["slot_suspect", "needs_you"]);
}

#[test]
fn a_stop_kills_a_seat_three_times_then_records_it_unkillable() {
    let mut stop = seat();
    stop.confirmed = true;
    stop.stopping = true;
    stop.stop_kills = STOP_KILLS - 1;
    assert_eq!(kinds(&decide_seat(&stop, at(10))), ["stop_kill"]);
    stop.stop_kills = STOP_KILLS;
    assert_eq!(
        kinds(&decide_seat(&stop, at(10))),
        ["record_unkillable", "needs_you"]
    );
    stop.process_gone = true;
    assert!(decide_seat(&stop, at(10)).is_empty());
}

#[tokio::test]
async fn a_pass_applies_what_it_decided_and_leaves_out_what_failed() -> anyhow::Result<()> {
    let effects = Recorder {
        refuse_kill: true,
        ..Recorder::default()
    };
    let done = run(&effects, &FakeClock(at(95)), &[seat()], &[], &[]).await;
    assert_eq!(kinds(&done), ["record_launch_failed", "relaunch"]);
    let applied = effects
        .applied
        .lock()
        .map_err(|_| anyhow::anyhow!("recorder poisoned"))?
        .len();
    assert_eq!(applied, 2);
    Ok(())
}

fn event(kind: EventKind, at: DateTime<Utc>, payload: serde_json::Value) -> FleetEvent {
    FleetEvent {
        id: 1,
        run_id: "run".to_string(),
        task: Some("T10".to_string()),
        session_uuid: Some(Uuid::from_u128(10)),
        kind,
        payload,
        occurred_at: at,
    }
}

#[test]
fn the_seat_facts_are_folded_from_the_event_record() {
    let events = vec![
        event(
            EventKind::SeatLaunchRequested,
            at(5),
            json!({"launch_id": 7}),
        ),
        event(
            EventKind::SeatLaunchFailed,
            at(100),
            json!({"launch_id": 6, "reason": format!("{UNCONFIRMED_PREFIX} 90 seconds")}),
        ),
        event(
            EventKind::SeatWoken,
            at(200),
            json!({"via": "remote_control"}),
        ),
        event(EventKind::SeatClosed, at(210), json!({"reason": STOP_KILL})),
    ];
    let facts = seat_facts("T10", Uuid::from_u128(10), &events, &Observed::default());
    let Some(facts) = facts else {
        panic!("a requested launch gives facts");
    };
    assert_eq!(facts.launch_id, 7);
    assert_eq!(facts.requested_at, at(5));
    assert_eq!(facts.relaunches, 1);
    assert_eq!(facts.woken_at, Some(at(200)));
    assert_eq!(facts.stop_kills, 1);
    assert!(!facts.closed && !facts.confirmed);
    let none = seat_facts("T10", Uuid::from_u128(11), &[], &Observed::default());
    assert!(none.is_none());
}

#[test]
fn a_timeout_is_told_from_an_ordinary_failure() {
    assert!(timed_out(&(
        None,
        "the check was stopped after 5 seconds".to_string()
    )));
    assert!(!timed_out(&(Some(1), "stopped after nothing".to_string())));
    assert!(!timed_out(&(None, "could not start".to_string())));
}
