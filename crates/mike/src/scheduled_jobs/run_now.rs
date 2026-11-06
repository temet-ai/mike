use std::sync::{Arc, LazyLock};

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use serde::Serialize;
use tokio::sync::Notify;

use super::jobs::{JOBS, Job};
use super::status::{self, JobStatus};

#[cfg(test)]
#[path = "run_now_tests.rs"]
mod run_now_tests;

static WAKE: LazyLock<DashMap<&'static str, Arc<Notify>>> = LazyLock::new(DashMap::new);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum RefusalReason {
    UnknownJob,
    NotStarted,
    TurnedOff,
    NoScripts,
    HeldElsewhere,
    AlreadyRunning,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct RunRefusal {
    pub(crate) reason: RefusalReason,
    pub(crate) message: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(crate) struct RunAccepted {
    pub(crate) job: &'static str,
    pub(crate) run_requested_at: DateTime<Utc>,
    pub(crate) already_requested: bool,
}

fn refused(reason: RefusalReason, message: String) -> RunRefusal {
    RunRefusal { reason, message }
}

pub(super) fn wake(name: &'static str) -> Arc<Notify> {
    WAKE.entry(name).or_default().clone()
}

fn utc_text(at: DateTime<Utc>) -> String {
    at.format("%Y-%m-%d %H:%M:%S UTC").to_string()
}

pub(super) fn refusal(job: &Job, status: &JobStatus) -> Option<RunRefusal> {
    if !status.started || status.root.is_none() {
        let why = format!(
            "{} cannot run now: its worker has not started in this proxy (no working directory, or start-up has not reached the workers)",
            job.name
        );
        return Some(refused(RefusalReason::NotStarted, why));
    }
    if !status.enabled {
        let why = format!(
            "{} is turned off by the {} setting; turn it on in /settings under Scheduled jobs to run it",
            job.name, job.enabled_key
        );
        return Some(refused(RefusalReason::TurnedOff, why));
    }
    if status.script_dir_missing {
        let why = format!(
            "{} cannot run on this machine: the job is the script {}, and mike was started in a directory that has no scripts folder",
            job.name, job.script
        );
        return Some(refused(RefusalReason::NoScripts, why));
    }
    if status.held_elsewhere {
        let why = format!(
            "{} runs on the machine that holds its lease, not this one; start it from that machine's mike",
            job.name
        );
        return Some(refused(RefusalReason::HeldElsewhere, why));
    }
    status.running_since.map(|since| {
        let why = format!(
            "{} is already running (started {}); wait for it to finish",
            job.name,
            utc_text(since)
        );
        refused(RefusalReason::AlreadyRunning, why)
    })
}

fn unknown(name: &str) -> RunRefusal {
    let known: Vec<&str> = JOBS.iter().map(|job| job.name).collect();
    refused(
        RefusalReason::UnknownJob,
        format!(
            "no scheduled job is named '{name}'; known jobs: {}",
            known.join(", ")
        ),
    )
}

pub(crate) fn request_run(name: &str, now: DateTime<Utc>) -> Result<RunAccepted, RunRefusal> {
    let job = JOBS
        .iter()
        .find(|job| job.name == name)
        .ok_or_else(|| unknown(name))?;
    request_job_run(job, now)
}

pub(super) fn request_job_run(
    job: &'static Job,
    now: DateTime<Utc>,
) -> Result<RunAccepted, RunRefusal> {
    let current = status::snapshot(job.name);
    if let Some(refusal) = refusal(job, &current) {
        return Err(refusal);
    }
    status::update(job.name, |s| {
        s.run_requested_at = s.run_requested_at.or(Some(now));
    });
    wake(job.name).notify_one();
    Ok(RunAccepted {
        job: job.name,
        run_requested_at: current.run_requested_at.unwrap_or(now),
        already_requested: current.run_requested_at.is_some(),
    })
}

pub(super) fn take_request(name: &'static str, now: DateTime<Utc>) -> bool {
    if status::snapshot(name).run_requested_at.is_none() {
        return false;
    }
    status::update(name, |s| {
        s.run_requested_at = None;
        s.running_since = Some(now);
    });
    true
}

pub(super) fn drop_request(job: &Job, why: &str) {
    if status::snapshot(job.name).run_requested_at.is_none() {
        return;
    }
    status::update(job.name, |s| s.run_requested_at = None);
    tracing::info!(
        job = job.name,
        why,
        "scheduled_jobs: dropped a manual run request"
    );
}
