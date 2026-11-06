use chrono::{DateTime, TimeZone, Utc};

use super::super::jobs::PROMOTE;
use super::*;

fn at(hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 14, hour, minute, 0)
        .single()
        .unwrap_or_default()
}

fn record(trigger: Trigger, started: DateTime<Utc>, result: RunResult) -> RunRecord {
    RunRecord {
        trigger,
        started_at: started,
        finished_at: started + chrono::Duration::seconds(40),
        duration_ms: 40_000,
        result,
        tail: vec!["first".to_owned(), "second".to_owned()],
    }
}

fn outcome(record: &RunRecord, failures: u32, retry_after: Option<DateTime<Utc>>) -> Outcome<'_> {
    Outcome {
        machine: None,
        record,
        consecutive_failures: failures,
        retry_after,
    }
}

#[test]
fn every_result_survives_a_round_trip_through_its_row() {
    for result in [
        RunResult::Succeeded,
        RunResult::Alarmed(1),
        RunResult::Exited(3),
        RunResult::Signalled,
        RunResult::TimedOut(PROMOTE.timeout),
        RunResult::Unrunnable("scripts/learning-promote does not exist".to_owned()),
    ] {
        let original = record(Trigger::Manual, at(21, 30), result);
        let stored = row(&PROMOTE, &outcome(&original, 0, None));
        assert_eq!(record_from_row(&PROMOTE, &stored), original);
    }
}

#[test]
fn a_row_carries_the_trigger_exit_code_and_joined_tail() {
    let original = record(Trigger::Schedule, at(21, 30), RunResult::Exited(2));
    let stored = row(&PROMOTE, &outcome(&original, 2, Some(at(22, 40))));
    assert_eq!(stored.trigger, "schedule");
    assert_eq!(stored.result_kind, "exited");
    assert_eq!(stored.exit_code, Some(2));
    assert_eq!(stored.output_tail, "first\nsecond");
    assert_eq!(stored.consecutive_failures, 2);
    assert_eq!(stored.retry_after, Some(at(22, 40)));
}

#[test]
fn restore_takes_the_slot_from_the_scheduled_run_and_backoff_from_the_latest() {
    let scheduled = record(Trigger::Schedule, at(21, 30), RunResult::Exited(2));
    let manual = record(Trigger::Manual, at(21, 50), RunResult::Exited(2));
    let runs = LatestJobRuns {
        latest: Some(row(&PROMOTE, &outcome(&manual, 2, Some(at(22, 40))))),
        last_scheduled_start: Some(scheduled.started_at),
    };
    let restored = restore(&PROMOTE, &runs);
    assert_eq!(restored.last_run, Some(manual));
    assert_eq!(restored.last_scheduled_started_at, Some(at(21, 30)));
    assert_eq!(restored.schedule.consecutive_failures(), 2);
    assert_eq!(restored.schedule.retry_after(), Some(at(22, 40)));
    assert!(!restored.schedule.due(&PROMOTE.cadence, at(21, 55)));
    assert!(!restored.schedule.due(&PROMOTE.cadence, at(22, 30)));
    assert!(restored.schedule.due(&PROMOTE.cadence, at(22, 40)));
}

#[test]
fn restore_without_history_is_a_fresh_schedule() {
    let restored = restore(&PROMOTE, &LatestJobRuns::default());
    assert_eq!(restored.last_run, None);
    assert_eq!(restored.schedule.consecutive_failures(), 0);
    assert!(restored.schedule.due(&PROMOTE.cadence, at(21, 30)));
}
