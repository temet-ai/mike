use std::path::PathBuf;

use chrono::{DateTime, TimeZone, Utc};

use super::super::jobs::PROMOTE;
use super::*;

static QUEUED: Job = Job {
    name: "run_now_test_queued",
    ..PROMOTE
};
static IDLE: Job = Job {
    name: "run_now_test_idle",
    ..PROMOTE
};

fn at(hour: u32, minute: u32) -> DateTime<Utc> {
    Utc.with_ymd_and_hms(2026, 10, 14, hour, minute, 0)
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

fn reason_for(status: &JobStatus) -> Option<RefusalReason> {
    refusal(&PROMOTE, status).map(|refusal| refusal.reason)
}

#[test]
fn refusal_while_the_job_runs_names_when_it_started() -> anyhow::Result<()> {
    let mut status = ready();
    status.running_since = Some(at(21, 30));
    let Some(refusal) = refusal(&PROMOTE, &status) else {
        anyhow::bail!("a running job must be refused");
    };
    assert_eq!(refusal.reason, RefusalReason::AlreadyRunning);
    assert!(
        refusal.message.contains("already running"),
        "{}",
        refusal.message
    );
    assert!(
        refusal.message.contains("2026-10-14 21:30:00 UTC"),
        "{}",
        refusal.message
    );
    Ok(())
}

#[test]
fn refusal_names_the_setting_when_the_job_is_turned_off() -> anyhow::Result<()> {
    let mut status = ready();
    status.enabled = false;
    let Some(refusal) = refusal(&PROMOTE, &status) else {
        anyhow::bail!("a turned-off job must be refused");
    };
    assert_eq!(refusal.reason, RefusalReason::TurnedOff);
    assert!(
        refusal.message.contains("turned off"),
        "{}",
        refusal.message
    );
    assert!(
        refusal.message.contains(PROMOTE.enabled_key),
        "{}",
        refusal.message
    );
    Ok(())
}

#[test]
fn refusal_names_the_script_when_the_directory_has_no_scripts_folder() -> anyhow::Result<()> {
    let mut status = ready();
    status.script_dir_missing = true;
    let Some(refusal) = refusal(&PROMOTE, &status) else {
        anyhow::bail!("a job without a scripts folder was not refused");
    };
    assert_eq!(refusal.reason, RefusalReason::NoScripts);
    assert!(
        refusal.message.contains("scripts/learning-promote"),
        "{}",
        refusal.message
    );
    Ok(())
}

#[test]
fn refusal_covers_unstarted_and_leased_elsewhere_jobs() {
    assert_eq!(
        reason_for(&JobStatus::default()),
        Some(RefusalReason::NotStarted)
    );
    let mut elsewhere = ready();
    elsewhere.held_elsewhere = true;
    assert_eq!(reason_for(&elsewhere), Some(RefusalReason::HeldElsewhere));
    assert_eq!(reason_for(&ready()), None);
}

#[test]
fn request_for_an_unknown_job_names_it_and_the_known_jobs() -> anyhow::Result<()> {
    let Err(refusal) = request_run("no_such_job", at(21, 30)) else {
        anyhow::bail!("an unknown job must be refused");
    };
    assert_eq!(refusal.reason, RefusalReason::UnknownJob);
    assert!(
        refusal.message.contains("'no_such_job'"),
        "{}",
        refusal.message
    );
    assert!(
        refusal.message.contains("cargo_sweep"),
        "{}",
        refusal.message
    );
    Ok(())
}

#[test]
fn request_is_queued_once_then_taken_by_the_worker_and_refused_while_running() -> anyhow::Result<()>
{
    status::update(QUEUED.name, |s| *s = ready());
    let first = request_job_run(&QUEUED, at(21, 30)).map_err(|r| anyhow::anyhow!(r.message))?;
    assert!(!first.already_requested);
    assert_eq!(first.run_requested_at, at(21, 30));
    let again = request_job_run(&QUEUED, at(21, 31)).map_err(|r| anyhow::anyhow!(r.message))?;
    assert!(again.already_requested);
    assert_eq!(again.run_requested_at, at(21, 30));
    assert!(take_request(QUEUED.name, at(21, 32)));
    let taken = status::snapshot(QUEUED.name);
    assert_eq!(taken.run_requested_at, None);
    assert_eq!(taken.running_since, Some(at(21, 32)));
    let Err(refusal) = request_job_run(&QUEUED, at(21, 33)) else {
        anyhow::bail!("a running job must refuse another request");
    };
    assert_eq!(refusal.reason, RefusalReason::AlreadyRunning);
    Ok(())
}

#[test]
fn take_request_without_a_request_is_false_and_changes_nothing() {
    status::update(IDLE.name, |s| *s = ready());
    assert!(!take_request(IDLE.name, at(21, 30)));
    assert_eq!(status::snapshot(IDLE.name), ready());
}
