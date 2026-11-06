use std::os::unix::fs::PermissionsExt;
use std::path::PathBuf;

use chrono::Utc;
use tokio_util::sync::CancellationToken;

use super::jobs::{Job, PROMOTE};
use super::schedule::Cadence;
use super::status::{self, JobStatus, Trigger};
use super::{Schedule, Worker, run_now};

const NEVER_OPEN: Cadence = Cadence {
    hours: &[],
    ..PROMOTE.cadence
};

static MANUAL_ONLY: Job = Job {
    name: "worker_test_manual_only",
    cadence: NEVER_OPEN,
    ..PROMOTE
};
static TURNED_OFF: Job = Job {
    name: "worker_test_turned_off",
    cadence: NEVER_OPEN,
    ..PROMOTE
};
static NO_SCRIPTS: Job = Job {
    name: "worker_test_no_scripts",
    cadence: NEVER_OPEN,
    ..PROMOTE
};

fn checkout(body: &str) -> anyhow::Result<tempfile::TempDir> {
    let dir = tempfile::tempdir()?;
    let path = dir.path().join(PROMOTE.script);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(&path, format!("#!/bin/sh\n{body}\n"))?;
    std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755))?;
    Ok(dir)
}

fn worker(job: &'static Job, root: PathBuf, enabled: bool) -> Worker {
    status::update(job.name, |s| {
        *s = JobStatus {
            started: true,
            enabled,
            root: Some(root.clone()),
            ..JobStatus::default()
        };
    });
    Worker {
        job,
        root,
        persist: None,
        machine: None,
        shutdown: CancellationToken::new(),
        schedule: Schedule::default(),
        enabled,
    }
}

#[tokio::test]
async fn tick_after_a_run_now_request_runs_the_job_once_as_manual() -> anyhow::Result<()> {
    let dir = checkout("echo ran by hand")?;
    let mut worker = worker(&MANUAL_ONLY, dir.path().to_path_buf(), true);
    run_now::request_job_run(&MANUAL_ONLY, Utc::now()).map_err(|r| anyhow::anyhow!(r.message))?;
    worker.tick().await;
    let after = status::snapshot(MANUAL_ONLY.name);
    let Some(run) = after.last_run.clone() else {
        anyhow::bail!("the requested run was not recorded: {after:?}");
    };
    assert_eq!(run.trigger, Trigger::Manual);
    assert_eq!(run.tail, vec!["ran by hand".to_owned()]);
    assert_eq!(after.run_requested_at, None);
    assert_eq!(after.running_since, None);
    assert_eq!(after.last_scheduled_started_at, None);
    assert_eq!(after.last_started_at, Some(run.started_at));
    worker.tick().await;
    assert_eq!(status::snapshot(MANUAL_ONLY.name).last_run, Some(run));
    Ok(())
}

#[tokio::test]
async fn tick_for_a_turned_off_job_drops_the_request_without_running() -> anyhow::Result<()> {
    let dir = checkout("echo should not run")?;
    let mut worker = worker(&TURNED_OFF, dir.path().to_path_buf(), true);
    run_now::request_job_run(&TURNED_OFF, Utc::now()).map_err(|r| anyhow::anyhow!(r.message))?;
    worker.enabled = false;
    worker.tick().await;
    let after = status::snapshot(TURNED_OFF.name);
    assert_eq!(after.run_requested_at, None);
    assert_eq!(after.last_run, None);
    assert!(!after.enabled);
    Ok(())
}

#[tokio::test]
async fn tick_without_a_scripts_folder_drops_the_request_without_running() -> anyhow::Result<()> {
    let dir = tempfile::tempdir()?;
    let mut worker = worker(&NO_SCRIPTS, dir.path().to_path_buf(), true);
    run_now::request_job_run(&NO_SCRIPTS, Utc::now()).map_err(|r| anyhow::anyhow!(r.message))?;
    worker.tick().await;
    let after = status::snapshot(NO_SCRIPTS.name);
    assert!(after.script_dir_missing);
    assert_eq!(after.run_requested_at, None);
    assert_eq!(after.last_run, None);
    assert_eq!(after.last_started_at, None);
    Ok(())
}

#[test]
fn first_tick_after_spreads_every_job_across_one_tick() {
    let offsets: Vec<std::time::Duration> = (0..super::JOBS.len())
        .map(super::first_tick_after)
        .collect();
    assert_eq!(offsets.first().copied(), Some(std::time::Duration::ZERO));
    assert!(offsets.windows(2).all(|pair| pair[0] < pair[1]));
    assert!(offsets.iter().all(|offset| *offset < super::TICK));
}
