mod history;
mod jobs;
mod lease;
mod run_now;
mod runner;
mod schedule;
mod status;
mod view;

use std::path::PathBuf;
use std::time::Duration;

use chrono::{DateTime, Utc};
use tokio::time::{Instant, MissedTickBehavior, interval_at};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

use crate::state::AppState;
use crate::store::persistence::{PersistError, PersistHandle};

pub(crate) use jobs::{JOBS, Job};
pub(crate) use run_now::{RefusalReason, RunRefusal, request_run};
pub(crate) use status::snapshot;
#[cfg(test)]
pub(crate) use status::{JobStatus, replace_for_tests};
pub(crate) use view::{JobState, JobView, build_view};

use runner::{RunReport, RunResult, run_job, script_dir_missing};
use schedule::Schedule;
use status::{RunRecord, Trigger};

#[cfg(test)]
#[path = "worker_tests.rs"]
mod worker_tests;

const TICK: Duration = Duration::from_secs(60);

struct Worker {
    job: &'static Job,
    root: PathBuf,
    persist: Option<PersistHandle>,
    machine: Option<Uuid>,
    shutdown: CancellationToken,
    schedule: Schedule,
    enabled: bool,
}

pub(crate) fn spawn_scheduled_jobs(state: &AppState) {
    let root = match std::env::current_dir() {
        Ok(root) => Some(root),
        Err(e) => {
            log_no_working_directory(&e);
            None
        }
    };
    let now = Utc::now();
    for (index, job) in JOBS.iter().enumerate() {
        let next = Schedule::default().next_run_at(&job.cadence, now);
        let no_scripts = root
            .as_ref()
            .is_some_and(|root| script_dir_missing(root, job));
        status::update(job.name, |s| {
            s.started = true;
            s.process_started_at = Some(now);
            s.enabled = true;
            s.root = root.clone();
            s.script_dir_missing = no_scripts;
            s.next_run_at = next;
        });
        if let Some(root) = &root {
            let mut worker = Worker {
                job,
                root: root.clone(),
                persist: state.persist().cloned(),
                machine: None,
                shutdown: state.shutdown(),
                schedule: Schedule::default(),
                enabled: true,
            };
            let first_tick = first_tick_after(index);
            state
                .tracker
                .spawn(async move { worker.run(first_tick).await });
            log_spawned(job, root);
        }
    }
}

fn first_tick_after(index: usize) -> Duration {
    let slots = u32::try_from(JOBS.len()).unwrap_or(u32::MAX).max(1);
    let slot = u32::try_from(index).unwrap_or(0) % slots;
    TICK / slots * slot
}

fn log_spawned(job: &Job, root: &std::path::Path) {
    tracing::info!(job = job.name, root = %root.display(), "scheduled_jobs: spawned");
}

fn log_no_working_directory(e: &std::io::Error) {
    tracing::warn!(error = %e, "scheduled_jobs: the working directory is unknown — not running");
}

fn log_setting_unreadable(job: &Job, e: &PersistError) {
    tracing::debug!(job = job.name, error = %e, "scheduled_jobs: could not read the enabled setting — keeping the last known value");
}

fn log_run(job: &Job, report: &RunReport, failures: u32, retry_after: Option<DateTime<Utc>>) {
    match report.result {
        RunResult::Succeeded => log_succeeded(job, report),
        RunResult::Alarmed(code) => log_alarmed(job, report, code),
        _ => log_failed(job, report, failures, retry_after),
    }
}

fn log_alarmed(job: &Job, report: &RunReport, code: i32) {
    let tail = report.tail.join("\n");
    let duration_secs = report.duration.as_secs();
    tracing::warn!(job = job.name, exit_code = code, duration_secs, %tail, "scheduled_jobs: run finished with an alarm");
}

fn log_succeeded(job: &Job, report: &RunReport) {
    let tail = report.tail.join("\n");
    let duration_secs = report.duration.as_secs();
    tracing::info!(job = job.name, duration_secs, %tail, "scheduled_jobs: run succeeded");
}

fn log_failed(job: &Job, report: &RunReport, failures: u32, retry_after: Option<DateTime<Utc>>) {
    let tail = report.tail.join("\n");
    let duration_secs = report.duration.as_secs();
    tracing::warn!(
        job = job.name,
        result = %report.result.describe(),
        duration_secs,
        consecutive_failures = failures,
        retry_after = ?retry_after,
        %tail,
        "scheduled_jobs: run failed"
    );
}

impl Worker {
    async fn run(&mut self, first_tick: Duration) {
        self.restore().await;
        let wake = run_now::wake(self.job.name);
        let mut ticker = interval_at(Instant::now() + first_tick, TICK);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                _ = self.shutdown.cancelled() => return,
                _ = wake.notified() => self.tick().await,
                _ = ticker.tick() => self.tick().await,
            }
        }
    }

    async fn restore(&mut self) {
        let Some(persist) = &self.persist else {
            return;
        };
        self.machine = persist.llm_machine_uuid().await;
        let Some(runs) = history::load(persist, self.job, self.machine).await else {
            return;
        };
        let restored = history::restore(self.job, &runs);
        self.schedule = restored.schedule;
        let failures = self.schedule.consecutive_failures();
        let retry_after = self.schedule.retry_after();
        let next = self.schedule.next_run_at(&self.job.cadence, Utc::now());
        status::update(self.job.name, |s| {
            s.last_started_at = restored.last_run.as_ref().map(|run| run.started_at);
            s.last_scheduled_started_at = restored.last_scheduled_started_at;
            s.last_run = restored.last_run.clone();
            s.consecutive_failures = failures;
            s.retry_after = retry_after;
            s.next_run_at = next;
        });
    }

    async fn tick(&mut self) {
        self.refresh_enabled().await;
        let no_scripts = self.refresh_script_dir();
        if !self.enabled {
            run_now::drop_request(self.job, "the job is turned off");
            return;
        }
        if no_scripts {
            run_now::drop_request(self.job, "the working directory has no scripts folder");
            return;
        }
        let holds = lease::holds(self.persist.as_ref(), self.job, TICK).await;
        status::update(self.job.name, |s| s.held_elsewhere = !holds);
        if !holds {
            run_now::drop_request(self.job, "another machine holds the job's lease");
            return;
        }
        let now = Utc::now();
        self.publish_next_run(now);
        if let Some(trigger) = self.trigger_at(now) {
            self.execute(trigger, now).await;
        }
    }

    fn trigger_at(&mut self, now: DateTime<Utc>) -> Option<Trigger> {
        if run_now::take_request(self.job.name, now) {
            return Some(Trigger::Manual);
        }
        if !self.schedule.due(&self.job.cadence, now) {
            return None;
        }
        self.schedule.begin(&self.job.cadence, now);
        Some(Trigger::Schedule)
    }

    async fn execute(&mut self, trigger: Trigger, started_at: DateTime<Utc>) {
        let scheduled = trigger == Trigger::Schedule;
        status::update(self.job.name, |s| {
            s.running_since = Some(started_at);
            s.last_started_at = Some(started_at);
            if scheduled {
                s.last_scheduled_started_at = Some(started_at);
            }
        });
        let report = tokio::select! {
            _ = self.shutdown.cancelled() => return,
            report = run_job(&self.root, self.job) => report,
        };
        self.record(report, trigger, started_at).await;
    }

    fn publish_next_run(&self, now: DateTime<Utc>) {
        let next = self.schedule.next_run_at(&self.job.cadence, now);
        status::update(self.job.name, |s| s.next_run_at = next);
    }

    fn refresh_script_dir(&self) -> bool {
        let missing = script_dir_missing(&self.root, self.job);
        status::update(self.job.name, |s| s.script_dir_missing = missing);
        missing
    }

    async fn refresh_enabled(&mut self) {
        if let Some(persist) = &self.persist {
            match persist.setting_value(self.job.enabled_key).await {
                Ok(value) => self.enabled = value.and_then(|v| v.as_bool()).unwrap_or(true),
                Err(e) => log_setting_unreadable(self.job, &e),
            }
        }
        let enabled = self.enabled;
        status::update(self.job.name, |s| s.enabled = enabled);
    }

    async fn record(&mut self, report: RunReport, trigger: Trigger, started_at: DateTime<Utc>) {
        let finished_at = Utc::now();
        let succeeded = report.result.completed();
        match trigger {
            Trigger::Schedule => self.schedule.finish(finished_at, succeeded),
            Trigger::Manual => self.schedule.finish_manual(finished_at, succeeded),
        }
        let failures = self.schedule.consecutive_failures();
        let retry_after = self.schedule.retry_after();
        let next = self.schedule.next_run_at(&self.job.cadence, finished_at);
        log_run(self.job, &report, failures, retry_after);
        let record = RunRecord {
            trigger,
            started_at,
            finished_at,
            duration_ms: u64::try_from(report.duration.as_millis()).unwrap_or(u64::MAX),
            result: report.result,
            tail: report.tail,
        };
        status::update(self.job.name, |s| {
            s.running_since = None;
            s.last_run = Some(record.clone());
            s.consecutive_failures = failures;
            s.retry_after = retry_after;
            s.next_run_at = next;
        });
        if let Some(persist) = &self.persist {
            let outcome = history::Outcome {
                machine: self.machine,
                record: &record,
                consecutive_failures: failures,
                retry_after,
            };
            history::save(persist, self.job, &outcome).await;
        }
    }
}
