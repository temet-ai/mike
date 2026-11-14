use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use super::jobs::Job;
use super::runner::RunResult;
use super::status::{JobStatus, RunRecord, Trigger};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum JobState {
    Ok,
    Failing,
    Overdue,
    NeverRun,
    Off,
    Running,
    Unavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum JobScope {
    ThisMachine,
    OneMachine,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(crate) struct JobView {
    pub(crate) id: &'static str,
    pub(crate) label: &'static str,
    pub(crate) description: &'static str,
    pub(crate) schedule: &'static str,
    pub(crate) group: JobGroup,
    pub(crate) window: JobWindow,
    pub(crate) enabled_key: &'static str,
    pub(crate) scope: JobScope,
    pub(crate) owner_machine_uuid: Option<Uuid>,
    pub(crate) enabled: bool,
    pub(crate) state: JobState,
    pub(crate) running_since: Option<DateTime<Utc>>,
    pub(crate) run_requested_at: Option<DateTime<Utc>>,
    pub(crate) last_started_at: Option<DateTime<Utc>>,
    pub(crate) last_trigger: Option<Trigger>,
    pub(crate) last_finished_at: Option<DateTime<Utc>>,
    pub(crate) last_exit_code: Option<i32>,
    pub(crate) last_output_tail: Option<String>,
    pub(crate) next_run_at: Option<DateTime<Utc>>,
    pub(crate) consecutive_failures: u32,
    pub(crate) detail: String,
    pub(crate) fix: String,
}

struct Assessment {
    state: JobState,
    detail: String,
    fix: String,
}

impl Assessment {
    fn new(state: JobState, detail: impl Into<String>, fix: impl Into<String>) -> Self {
        Self {
            state,
            detail: detail.into(),
            fix: fix.into(),
        }
    }

    fn quiet(state: JobState) -> Self {
        Self::new(state, "", "")
    }
}

fn utc_text(at: DateTime<Utc>) -> String {
    at.format("%Y-%m-%d %H:%M UTC").to_string()
}

fn root_text(status: &JobStatus) -> String {
    status.root.as_ref().map_or_else(
        || "an unknown directory".to_owned(),
        |root| root.display().to_string(),
    )
}

fn failing(job: &Job, status: &JobStatus) -> Option<Assessment> {
    let last: &RunRecord = status.last_run.as_ref()?;
    if status.consecutive_failures == 0 {
        return None;
    }
    let next = status.retry_after.map_or_else(String::new, |at| {
        format!(" No new attempt before {}.", utc_text(at))
    });
    let output = if last.tail.is_empty() {
        "The script printed nothing.".to_owned()
    } else {
        format!("Last output lines: {}", last.tail.join("; "))
    };
    let detail = format!(
        "{} run(s) in a row failed. The latest, finished {}, {}.{next} {output}",
        status.consecutive_failures,
        utc_text(last.finished_at),
        last.result.describe(),
    );
    let fix = format!(
        "Run `{}` by hand in {} to see the failure. {}",
        job.script,
        root_text(status),
        job.failure_hint
    );
    Some(Assessment::new(JobState::Failing, detail, fix))
}

fn overdue(job: &Job, status: &JobStatus, now: DateTime<Utc>) -> Option<DateTime<Utc>> {
    let started = status
        .last_scheduled_started_at
        .or(status.last_started_at)?;
    let missed = job.cadence.next_due_after(started)?;
    let caught_up = status.last_started_at.is_some_and(|last| last > missed);
    (!caught_up && now - missed > job.cadence.interval()).then_some(missed)
}

fn missed_run(status: &JobStatus, due: DateTime<Utc>) -> Assessment {
    let fix = "Press Run now to catch up; the row then shows that run.";
    match status.process_started_at {
        Some(up) if up > due => Assessment::new(
            JobState::Overdue,
            format!(
                "A run was due at {} but mike was not running on this machine then; it started at {}.",
                utc_text(due),
                utc_text(up)
            ),
            fix,
        ),
        _ => Assessment::new(
            JobState::Overdue,
            format!(
                "A run was due at {} and did not start, although mike was running. The scheduler's lines around that time are in .deploy/mike.log (search scheduled_jobs).",
                utc_text(due)
            ),
            fix,
        ),
    }
}

fn alarmed(job: &Job, status: &JobStatus) -> Option<Assessment> {
    let last: &RunRecord = status.last_run.as_ref()?;
    let RunResult::Alarmed(code) = last.result else {
        return None;
    };
    let output = if last.tail.is_empty() {
        "The script printed nothing.".to_owned()
    } else {
        format!("Alarm output: {}", last.tail.join("; "))
    };
    let detail = format!(
        "The latest run, finished {}, raised an alarm (exit code {code}) and still wrote its report. {output}",
        utc_text(last.finished_at),
    );
    let fix = format!(
        "Read the alarm lines in the detail, then run `{}` by hand in {} for the full report.",
        job.script,
        root_text(status)
    );
    Some(Assessment::new(JobState::Ok, detail, fix))
}

fn owned_elsewhere(owner: Option<Uuid>) -> Assessment {
    let detail = owner.map_or_else(
        || "Another machine owns this job and runs it; this machine does not.".to_owned(),
        |machine| format!("Machine {machine} owns this job and runs it; this machine does not."),
    );
    Assessment::new(JobState::Ok, detail, "")
}

fn unavailable(job: &Job, status: &JobStatus) -> Assessment {
    let detail = format!(
        "This job is the script {}, which mike runs from the directory it was started in. {} has no scripts folder, so the job does not run on this machine.",
        job.script,
        root_text(status)
    );
    Assessment::new(JobState::Unavailable, detail, "")
}

fn gate(job: &Job, status: &JobStatus, owner: Option<Uuid>) -> Option<Assessment> {
    if !status.started {
        return Some(Assessment::new(
            JobState::Off,
            format!(
                "The {} job has not started in this process (smoke run, or start-up has not reached the workers).",
                job.label
            ),
            "",
        ));
    }
    if status.root.is_none() {
        return Some(Assessment::new(
            JobState::Failing,
            format!(
                "The proxy's working directory is unknown, so it cannot find {}.",
                job.script
            ),
            "Start the proxy from the mike checkout (the stack unit sets WorkingDirectory) and check the proxy log.",
        ));
    }
    if !status.enabled {
        return Some(Assessment::new(
            JobState::Off,
            format!(
                "{} is switched off by the {} setting.",
                job.label, job.enabled_key
            ),
            format!(
                "Turn on the {} setting in /settings under Scheduled jobs.",
                job.enabled_key
            ),
        ));
    }
    if status.script_dir_missing {
        return Some(match owner.filter(|_| job.leased) {
            Some(machine) => owned_elsewhere(Some(machine)),
            None => unavailable(job, status),
        });
    }
    status.held_elsewhere.then(|| owned_elsewhere(owner))
}

fn assess(job: &Job, status: &JobStatus, owner: Option<Uuid>, now: DateTime<Utc>) -> Assessment {
    if let Some(blocked) = gate(job, status, owner) {
        return blocked;
    }
    if status.running_since.is_some() {
        return Assessment::quiet(JobState::Running);
    }
    if let Some(failed) = failing(job, status) {
        return failed;
    }
    if status.last_started_at.is_none() {
        return Assessment::quiet(JobState::NeverRun);
    }
    match overdue(job, status, now) {
        Some(due) => missed_run(status, due),
        None => alarmed(job, status).unwrap_or_else(|| Assessment::quiet(JobState::Ok)),
    }
}

pub(crate) fn build_view(
    job: &'static Job,
    status: &JobStatus,
    owner: Option<Uuid>,
    now: DateTime<Utc>,
) -> JobView {
    let assessment = assess(job, status, owner, now);
    let scheduled_here = status.enabled && !status.held_elsewhere && !status.script_dir_missing;
    let scope = if job.leased {
        JobScope::OneMachine
    } else {
        JobScope::ThisMachine
    };
    JobView {
        id: job.name,
        label: job.label,
        description: job.description,
        schedule: job.schedule_text,
        group: window::group_of(job),
        window: window::window_of(job),
        enabled_key: job.enabled_key,
        scope,
        owner_machine_uuid: if job.leased { owner } else { None },
        enabled: status.enabled,
        state: assessment.state,
        running_since: status.running_since,
        run_requested_at: status.run_requested_at,
        last_started_at: status.last_started_at,
        last_trigger: status.last_run.as_ref().map(|run| run.trigger),
        last_finished_at: status.last_run.as_ref().map(|run| run.finished_at),
        last_exit_code: status
            .last_run
            .as_ref()
            .and_then(|run| run.result.exit_code()),
        last_output_tail: status.last_run.as_ref().map(|run| run.tail.join("\n")),
        next_run_at: if scheduled_here {
            status.next_run_at
        } else {
            None
        },
        consecutive_failures: status.consecutive_failures,
        detail: assessment.detail,
        fix: assessment.fix,
    }
}

#[cfg(test)]
mod tests;
mod window;

pub(crate) use window::{JobGroup, JobWindow};
