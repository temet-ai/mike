use chrono::{Duration, TimeZone, Utc};
use serde_json::json;

use super::{Outcome, Skip, gate, mark_running, record_ran, record_skip, seed, snapshot};

fn noon() -> chrono::DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 11, 13, 12, 0, 0)
        .single()
        .unwrap_or_else(Utc::now)
}

#[test]
fn gate_outside_window_after_a_day_without_a_pass_and_idle_runs() {
    let ran = Some(noon() - Duration::hours(25));
    assert_eq!(gate(false, ran, noon(), true), None);
}

#[test]
fn gate_outside_window_with_no_pass_ever_and_idle_runs() {
    assert_eq!(gate(false, None, noon(), true), None);
}

#[test]
fn gate_outside_window_after_a_pass_within_a_day_is_outside_window() {
    let ran = Some(noon() - Duration::hours(23));
    assert_eq!(gate(false, ran, noon(), true), Some(Skip::OutsideWindow));
}

#[test]
fn gate_catching_up_while_busy_is_busy() {
    let ran = Some(noon() - Duration::hours(30));
    assert_eq!(gate(false, ran, noon(), false), Some(Skip::Busy));
}

#[test]
fn gate_inside_window_and_idle_runs_even_after_a_recent_pass() {
    let ran = Some(noon() - Duration::hours(1));
    assert_eq!(gate(true, ran, noon(), true), None);
}

#[test]
fn seed_keeps_the_newer_of_the_recorded_run_and_the_newest_row() {
    let worker = "test_seed_worker";
    record_ran(worker, noon(), &json!({}));
    seed(worker, Some(noon() - Duration::days(3)));
    let status = snapshot(worker);
    assert!(status.seeded);
    assert_eq!(status.last_ran_at, Some(noon()));
}

#[test]
fn mark_running_shows_until_the_pass_is_recorded() {
    let worker = "test_running_worker";
    mark_running(worker, noon());
    assert_eq!(snapshot(worker).running_since, Some(noon()));
    record_ran(worker, noon() + Duration::minutes(2), &json!({}));
    assert_eq!(snapshot(worker).running_since, None);
}

#[test]
fn record_skip_keeps_the_last_ran_at_and_names_the_reason() {
    let worker = "test_skip_worker";
    record_ran(worker, noon(), &json!({"judged": 3}));
    record_skip(worker, noon() + Duration::minutes(10), Skip::NotLeader);
    let status = snapshot(worker);
    assert_eq!(status.last_ran_at, Some(noon()));
    assert_eq!(
        status.last_outcome,
        Some(Outcome::Skipped {
            reason: Skip::NotLeader
        })
    );
    assert_eq!(
        serde_json::to_value(&status.last_outcome).ok(),
        Some(json!({"outcome": "skipped", "reason": "not_leader"}))
    );
}
