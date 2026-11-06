use super::super::jobs::{AUDIT, CARGO_SWEEP, PROMOTE, USAGE_AUDIT};
use super::*;
use crate::scheduled_jobs::status::Trigger;
use chrono::TimeZone;
use std::path::PathBuf;

fn at(day: u32, hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, day, hour, minute, 0)
        .single()
        .unwrap_or_default()
}

fn ready() -> JobStatus {
    JobStatus {
        started: true,
        enabled: true,
        root: Some(PathBuf::from("/srv/mike")),
        ..JobStatus::default()
    }
}

fn ran(finished: DateTime<Utc>, result: RunResult) -> RunRecord {
    RunRecord {
        trigger: Trigger::Schedule,
        started_at: finished - chrono::Duration::seconds(42),
        finished_at: finished,
        duration_ms: 42_000,
        result,
        tail: vec!["Traceback".to_owned(), "PermissionError".to_owned()],
    }
}

fn state_of(job: &'static Job, status: &JobStatus, now: DateTime<Utc>) -> JobState {
    build_view(job, status, None, now).state
}

#[test]
fn a_job_before_the_workers_start_is_off() {
    let v = build_view(&PROMOTE, &JobStatus::default(), None, at(14, 12, 0));
    assert_eq!(v.state, JobState::Off);
    assert!(!v.detail.is_empty());
}

#[test]
fn a_job_without_a_working_directory_is_failing() {
    let status = JobStatus {
        started: true,
        enabled: true,
        ..JobStatus::default()
    };
    let v = build_view(&PROMOTE, &status, None, at(14, 12, 0));
    assert_eq!(v.state, JobState::Failing);
    assert!(v.fix.contains("WorkingDirectory"), "{}", v.fix);
}

#[test]
fn a_switched_off_job_is_off_names_its_setting_and_has_no_next_run() {
    let mut status = ready();
    status.enabled = false;
    status.next_run_at = Some(at(14, 20, 30));
    let v = build_view(&AUDIT, &status, None, at(14, 12, 0));
    assert_eq!(v.state, JobState::Off);
    assert!(v.detail.contains("learning_audit.enabled"), "{}", v.detail);
    assert!(v.fix.contains("/settings"), "{}", v.fix);
    assert_eq!(v.next_run_at, None);
    assert!(!v.enabled);
}

#[test]
fn a_leased_job_owned_by_another_machine_is_ok_and_names_the_owner() {
    let mut status = ready();
    status.held_elsewhere = true;
    status.next_run_at = Some(at(15, 1, 0));
    let owner = Uuid::new_v4();
    let v = build_view(&AUDIT, &status, Some(owner), at(14, 12, 0));
    assert_eq!(v.state, JobState::Ok);
    assert!(v.enabled);
    assert_eq!(v.scope, JobScope::OneMachine);
    assert_eq!(v.owner_machine_uuid, Some(owner));
    assert!(v.detail.contains(&owner.to_string()), "{}", v.detail);
    assert_eq!(v.next_run_at, None);
}

#[test]
fn a_job_owned_elsewhere_with_an_unknown_owner_is_still_ok() {
    let mut status = ready();
    status.held_elsewhere = true;
    let v = build_view(&USAGE_AUDIT, &status, None, at(14, 12, 0));
    assert_eq!(v.state, JobState::Ok);
    assert_eq!(v.owner_machine_uuid, None);
    assert!(v.detail.contains("Another machine"), "{}", v.detail);
}

#[test]
fn a_switched_off_leased_job_stays_off_even_when_owned_elsewhere() {
    let mut status = ready();
    status.enabled = false;
    status.held_elsewhere = true;
    let v = build_view(&USAGE_AUDIT, &status, Some(Uuid::new_v4()), at(14, 12, 0));
    assert_eq!(v.state, JobState::Off);
}

#[test]
fn a_run_that_ended_with_an_alarm_is_ok_and_shows_the_alarm_in_the_detail() {
    let mut status = ready();
    status.last_started_at = Some(at(11, 6, 0));
    status.last_run = Some(RunRecord {
        trigger: Trigger::Schedule,
        started_at: at(11, 6, 0),
        finished_at: at(11, 6, 3),
        duration_ms: 180_000,
        result: RunResult::Alarmed(1),
        tail: vec![
            "cache re-write rate 3.1% exceeds 1.5%".to_owned(),
            "full report: /home/u/.local/state/mike-audit/2026-10-11.txt".to_owned(),
        ],
    });
    let v = build_view(&USAGE_AUDIT, &status, None, at(11, 7, 0));
    assert_eq!(v.state, JobState::Ok);
    assert_eq!(v.last_exit_code, Some(1));
    assert_eq!(v.consecutive_failures, 0);
    assert!(v.detail.contains("raised an alarm"), "{}", v.detail);
    assert!(
        v.detail.contains("cache re-write rate 3.1%"),
        "{}",
        v.detail
    );
    assert!(v.fix.contains("scripts/usage-audit.sh"), "{}", v.fix);
}

#[test]
fn a_run_that_ended_without_an_alarm_has_no_alarm_detail() {
    let mut status = ready();
    status.last_started_at = Some(at(11, 6, 0));
    status.last_run = Some(ran(at(11, 6, 3), RunResult::Succeeded));
    let v = build_view(&USAGE_AUDIT, &status, None, at(11, 7, 0));
    assert_eq!(v.state, JobState::Ok);
    assert!(v.detail.is_empty() && v.fix.is_empty());
}

#[test]
fn an_alarmed_job_that_misses_its_next_week_is_overdue() {
    let mut status = ready();
    status.last_started_at = Some(at(4, 6, 0));
    status.last_run = Some(RunRecord {
        trigger: Trigger::Schedule,
        started_at: at(4, 6, 0),
        finished_at: at(4, 6, 3),
        duration_ms: 180_000,
        result: RunResult::Alarmed(1),
        tail: Vec::new(),
    });
    assert_eq!(
        state_of(&USAGE_AUDIT, &status, at(30, 12, 0)),
        JobState::Overdue
    );
}

#[test]
fn a_per_machine_job_never_reports_an_owner() {
    let v = build_view(&PROMOTE, &ready(), Some(Uuid::new_v4()), at(14, 12, 0));
    assert_eq!(v.scope, JobScope::ThisMachine);
    assert_eq!(v.owner_machine_uuid, None);
}

#[test]
fn a_job_that_has_not_run_since_start_is_never_run_and_quiet() {
    let v = build_view(&PROMOTE, &ready(), None, at(14, 12, 0));
    assert_eq!(v.state, JobState::NeverRun);
    assert!(v.detail.is_empty() && v.fix.is_empty());
    assert_eq!(v.last_output_tail, None);
}

#[test]
fn a_job_that_is_executing_is_running() {
    let mut status = ready();
    status.running_since = Some(at(14, 21, 30));
    status.last_started_at = Some(at(14, 21, 30));
    assert_eq!(
        state_of(&PROMOTE, &status, at(14, 21, 40)),
        JobState::Running
    );
}

#[test]
fn a_job_whose_last_run_succeeded_is_ok_with_its_exit_code_and_output() {
    let mut status = ready();
    status.last_started_at = Some(at(14, 21, 30));
    status.last_run = Some(ran(at(14, 21, 35), RunResult::Succeeded));
    let v = build_view(&PROMOTE, &status, None, at(14, 21, 50));
    assert_eq!(v.state, JobState::Ok);
    assert_eq!(v.last_exit_code, Some(0));
    assert_eq!(v.last_finished_at, Some(at(14, 21, 35)));
    assert_eq!(v.last_started_at, Some(at(14, 21, 30)));
    assert_eq!(
        v.last_output_tail.as_deref(),
        Some("Traceback\nPermissionError")
    );
    assert!(v.detail.is_empty() && v.fix.is_empty());
}

#[test]
fn a_failed_run_is_failing_with_the_exit_code_output_and_a_fix() {
    let mut status = ready();
    status.last_started_at = Some(at(14, 21, 30));
    status.last_run = Some(ran(at(14, 21, 35), RunResult::Exited(1)));
    status.consecutive_failures = 1;
    status.retry_after = Some(at(14, 22, 5));
    let v = build_view(&PROMOTE, &status, None, at(14, 21, 50));
    assert_eq!(v.state, JobState::Failing);
    assert_eq!(v.last_exit_code, Some(1));
    assert_eq!(v.consecutive_failures, 1);
    assert!(v.detail.contains("exited with code 1"), "{}", v.detail);
    assert!(v.detail.contains("PermissionError"), "{}", v.detail);
    assert!(v.detail.contains("2026-10-14 22:05 UTC"), "{}", v.detail);
    assert!(v.fix.contains("scripts/learning-promote"), "{}", v.fix);
    assert!(v.fix.contains("conflict"), "{}", v.fix);
}

#[test]
fn a_timed_out_run_has_no_exit_code() {
    let mut status = ready();
    status.last_started_at = Some(at(14, 21, 30));
    status.last_run = Some(ran(
        at(14, 21, 45),
        RunResult::TimedOut(std::time::Duration::from_secs(900)),
    ));
    status.consecutive_failures = 3;
    let v = build_view(&PROMOTE, &status, None, at(14, 21, 50));
    assert_eq!(v.state, JobState::Failing);
    assert_eq!(v.last_exit_code, None);
    assert!(v.detail.contains("3 run(s) in a row"), "{}", v.detail);
}

#[test]
fn a_run_due_more_than_an_interval_ago_that_never_started_is_overdue() {
    let mut status = ready();
    status.last_started_at = Some(at(14, 20, 31));
    status.last_run = Some(ran(at(14, 20, 35), RunResult::Succeeded));
    assert_eq!(state_of(&PROMOTE, &status, at(14, 21, 45)), JobState::Ok);
    let v = build_view(&PROMOTE, &status, None, at(14, 22, 45));
    assert_eq!(v.state, JobState::Overdue);
    assert!(v.detail.contains("2026-10-14 21:30 UTC"), "{}", v.detail);
    assert!(!v.fix.is_empty());
}

#[test]
fn a_job_between_windows_is_not_overdue() {
    let mut status = ready();
    status.last_started_at = Some(at(14, 3, 31));
    status.last_run = Some(ran(at(14, 3, 35), RunResult::Succeeded));
    assert_eq!(state_of(&PROMOTE, &status, at(14, 12, 0)), JobState::Ok);
    let mut sweep = ready();
    sweep.last_started_at = Some(at(14, 6, 0));
    sweep.last_run = Some(ran(at(14, 6, 5), RunResult::Succeeded));
    assert_eq!(state_of(&CARGO_SWEEP, &sweep, at(14, 9, 0)), JobState::Ok);
    assert_eq!(
        state_of(&CARGO_SWEEP, &sweep, at(14, 13, 30)),
        JobState::Overdue
    );
}

#[test]
fn the_view_serializes_to_the_documented_shape() -> anyhow::Result<()> {
    let mut status = ready();
    status.last_started_at = Some(at(14, 21, 30));
    status.last_run = Some(ran(at(14, 21, 35), RunResult::Succeeded));
    status.next_run_at = Some(at(14, 22, 30));
    let value = serde_json::to_value(build_view(&AUDIT, &status, None, at(14, 21, 50)))?;
    for key in [
        "id",
        "label",
        "description",
        "schedule",
        "scope",
        "owner_machine_uuid",
        "enabled",
        "state",
        "running_since",
        "run_requested_at",
        "last_started_at",
        "last_trigger",
        "last_finished_at",
        "last_exit_code",
        "last_output_tail",
        "next_run_at",
        "consecutive_failures",
        "detail",
        "fix",
    ] {
        assert!(value.get(key).is_some(), "missing {key}: {value}");
    }
    assert_eq!(value["scope"], "one_machine");
    assert_eq!(value["state"], "ok");
    assert_eq!(value["next_run_at"], "2026-10-14T22:30:00Z");
    assert_eq!(value["last_trigger"], "schedule");
    assert!(value["running_since"].is_null(), "{value}");
    Ok(())
}

#[test]
fn a_running_job_reports_when_it_started_and_a_manual_trigger() -> anyhow::Result<()> {
    let mut status = ready();
    let mut manual = ran(at(14, 20, 0), RunResult::Succeeded);
    manual.trigger = Trigger::Manual;
    status.last_run = Some(manual);
    status.last_started_at = Some(at(14, 21, 30));
    status.running_since = Some(at(14, 21, 30));
    let v = build_view(&PROMOTE, &status, None, at(14, 21, 40));
    assert_eq!(v.state, JobState::Running);
    assert_eq!(v.running_since, Some(at(14, 21, 30)));
    let value = serde_json::to_value(&v)?;
    assert_eq!(value["last_trigger"], "manual");
    assert_eq!(value["running_since"], "2026-10-14T21:30:00Z");
    Ok(())
}

#[test]
fn a_manual_run_does_not_hide_a_missed_scheduled_run() {
    let mut status = ready();
    status.last_scheduled_started_at = Some(at(4, 6, 0));
    status.last_started_at = Some(at(30, 11, 0));
    status.last_run = Some(ran(at(30, 11, 1), RunResult::Succeeded));
    assert_eq!(
        state_of(&USAGE_AUDIT, &status, at(30, 12, 0)),
        JobState::Overdue
    );
}

fn without_scripts() -> JobStatus {
    JobStatus {
        script_dir_missing: true,
        ..ready()
    }
}

#[test]
fn a_job_without_a_scripts_folder_is_unavailable_and_not_scheduled() {
    let mut status = without_scripts();
    status.next_run_at = Some(at(14, 18, 0));
    let v = build_view(&CARGO_SWEEP, &status, None, at(14, 12, 0));
    assert_eq!(v.state, JobState::Unavailable);
    assert!(v.detail.contains("scripts/cargo-sweep-all"), "{}", v.detail);
    assert!(v.detail.contains("/srv/mike"), "{}", v.detail);
    assert!(v.fix.is_empty(), "{}", v.fix);
    assert_eq!(v.next_run_at, None);
}

#[test]
fn earlier_failed_runs_do_not_make_a_job_without_a_scripts_folder_failing() {
    let mut status = without_scripts();
    status.last_started_at = Some(at(14, 6, 0));
    status.last_run = Some(ran(
        at(14, 6, 0),
        RunResult::Unrunnable("scripts/cargo-sweep-all does not exist".to_owned()),
    ));
    status.consecutive_failures = 5;
    assert_eq!(
        state_of(&CARGO_SWEEP, &status, at(14, 12, 0)),
        JobState::Unavailable
    );
}

#[test]
fn a_leased_job_without_a_scripts_folder_names_the_machine_that_runs_it() {
    let owner = Uuid::from_u128(7);
    let v = build_view(&AUDIT, &without_scripts(), Some(owner), at(14, 12, 0));
    assert_eq!(v.state, JobState::Ok);
    assert!(v.detail.contains(&owner.to_string()), "{}", v.detail);
    let unowned = build_view(&AUDIT, &without_scripts(), None, at(14, 12, 0));
    assert_eq!(unowned.state, JobState::Unavailable);
}

#[test]
fn a_switched_off_job_without_a_scripts_folder_is_off() {
    let mut status = without_scripts();
    status.enabled = false;
    assert_eq!(state_of(&PROMOTE, &status, at(14, 12, 0)), JobState::Off);
}
