use chrono::{DateTime, TimeZone, Utc};

use super::super::jobs::{AUDIT, PROMOTE};
use super::Schedule;

fn at(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, day, hour, minute, 0)
        .single()
        .unwrap_or_default()
}

#[test]
fn restored_schedule_after_a_run_this_hour_does_not_run_the_hour_again() {
    let schedule = Schedule::restored(&PROMOTE.cadence, Some(at(14, 21, 30)), 0, None);
    assert!(!schedule.due(&PROMOTE.cadence, at(14, 21, 45)));
    assert!(schedule.due(&PROMOTE.cadence, at(14, 22, 30)));
}

#[test]
fn restored_schedule_after_this_weeks_audit_waits_for_next_monday() {
    let schedule = Schedule::restored(&AUDIT.cadence, Some(at(12, 1, 0)), 0, None);
    assert!(!schedule.due(&AUDIT.cadence, at(12, 3, 0)));
    assert!(schedule.due(&AUDIT.cadence, at(19, 1, 0)));
    assert_eq!(
        schedule.next_run_at(&AUDIT.cadence, at(12, 3, 0)),
        Some(at(19, 1, 0))
    );
}

#[test]
fn restored_schedule_keeps_the_backoff_of_the_last_failure() {
    let schedule = Schedule::restored(
        &PROMOTE.cadence,
        Some(at(14, 21, 30)),
        2,
        Some(at(14, 22, 40)),
    );
    assert_eq!(schedule.consecutive_failures(), 2);
    assert_eq!(schedule.retry_after(), Some(at(14, 22, 40)));
    assert!(!schedule.due(&PROMOTE.cadence, at(14, 22, 30)));
    assert!(schedule.due(&PROMOTE.cadence, at(14, 22, 40)));
    assert_eq!(
        schedule.next_run_at(&PROMOTE.cadence, at(14, 22, 31)),
        Some(at(14, 22, 40))
    );
}

#[test]
fn restored_schedule_with_no_scheduled_run_is_due_in_an_open_window() {
    let schedule = Schedule::restored(&PROMOTE.cadence, None, 0, None);
    assert!(schedule.due(&PROMOTE.cadence, at(14, 21, 30)));
}

#[test]
fn manual_failure_counts_but_does_not_push_the_schedule_back() {
    let mut schedule = Schedule::restored(&PROMOTE.cadence, Some(at(14, 20, 30)), 0, None);
    schedule.finish_manual(at(14, 21, 5), false);
    assert_eq!(schedule.consecutive_failures(), 1);
    assert_eq!(schedule.retry_after(), None);
    assert!(schedule.due(&PROMOTE.cadence, at(14, 21, 30)));
}

#[test]
fn manual_success_clears_failures_and_backoff_but_leaves_the_slot() {
    let mut schedule = Schedule::restored(
        &PROMOTE.cadence,
        Some(at(14, 21, 30)),
        3,
        Some(at(14, 23, 40)),
    );
    schedule.finish_manual(at(14, 21, 50), true);
    assert_eq!(schedule.consecutive_failures(), 0);
    assert_eq!(schedule.retry_after(), None);
    assert!(!schedule.due(&PROMOTE.cadence, at(14, 21, 55)));
    assert!(schedule.due(&PROMOTE.cadence, at(14, 22, 30)));
}
