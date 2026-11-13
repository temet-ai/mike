use crate::store::persistence::web_repo::WebPersist as _;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::jobs::Job;
use super::runner::RunResult;
use super::schedule::Schedule;
use super::status::{RunRecord, Trigger};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::scheduled_job_runs::{JobRunRow, LatestJobRuns};

#[cfg(test)]
#[path = "history_tests.rs"]
mod history_tests;

#[derive(Debug, Clone)]
pub(super) struct Restored {
    pub(super) schedule: Schedule,
    pub(super) last_run: Option<RunRecord>,
    pub(super) last_scheduled_started_at: Option<DateTime<Utc>>,
}

pub(super) struct Outcome<'a> {
    pub(super) machine: Option<Uuid>,
    pub(super) record: &'a RunRecord,
    pub(super) consecutive_failures: u32,
    pub(super) retry_after: Option<DateTime<Utc>>,
}

fn result_columns(result: &RunResult) -> (&'static str, Option<String>) {
    match result {
        RunResult::Succeeded => ("succeeded", None),
        RunResult::Alarmed(_) => ("alarmed", None),
        RunResult::Exited(_) => ("exited", None),
        RunResult::Signalled => ("signalled", None),
        RunResult::TimedOut(_) => ("timed_out", None),
        RunResult::Unrunnable(why) => ("unrunnable", Some(why.clone())),
    }
}

fn result_from_row(job: &Job, row: &JobRunRow) -> RunResult {
    let code = row.exit_code.unwrap_or_default();
    match row.result_kind.as_str() {
        "succeeded" => RunResult::Succeeded,
        "alarmed" => RunResult::Alarmed(code),
        "exited" => RunResult::Exited(code),
        "signalled" => RunResult::Signalled,
        "timed_out" => RunResult::TimedOut(job.timeout),
        other => RunResult::Unrunnable(
            row.result_detail
                .clone()
                .unwrap_or_else(|| format!("recorded as {other}")),
        ),
    }
}

pub(super) fn row(job: &Job, outcome: &Outcome<'_>) -> JobRunRow {
    let record = outcome.record;
    let (result_kind, result_detail) = result_columns(&record.result);
    JobRunRow {
        job_name: job.name.to_owned(),
        machine_uuid: outcome.machine,
        trigger: record.trigger.as_db().to_owned(),
        started_at: record.started_at,
        finished_at: record.finished_at,
        duration_ms: i64::try_from(record.duration_ms).unwrap_or(i64::MAX),
        result_kind: result_kind.to_owned(),
        exit_code: record.result.exit_code(),
        result_detail,
        output_tail: record.tail.join("\n"),
        consecutive_failures: i32::try_from(outcome.consecutive_failures).unwrap_or(i32::MAX),
        retry_after: outcome.retry_after,
    }
}

fn record_from_row(job: &Job, row: &JobRunRow) -> RunRecord {
    RunRecord {
        trigger: Trigger::from_db(&row.trigger),
        started_at: row.started_at,
        finished_at: row.finished_at,
        duration_ms: u64::try_from(row.duration_ms).unwrap_or_default(),
        result: result_from_row(job, row),
        tail: if row.output_tail.is_empty() {
            Vec::new()
        } else {
            row.output_tail.lines().map(str::to_owned).collect()
        },
    }
}

pub(super) fn restore(job: &Job, runs: &LatestJobRuns) -> Restored {
    let latest = runs.latest.as_ref();
    let failures = latest.map_or(0, |run| {
        u32::try_from(run.consecutive_failures).unwrap_or(0)
    });
    let retry_after = latest.and_then(|run| run.retry_after);
    Restored {
        schedule: Schedule::restored(
            &job.cadence,
            runs.last_scheduled_start,
            failures,
            retry_after,
        ),
        last_run: latest.map(|run| record_from_row(job, run)),
        last_scheduled_started_at: runs.last_scheduled_start,
    }
}

pub(super) async fn load(
    persist: &PersistHandle,
    job: &Job,
    machine: Option<Uuid>,
) -> Option<LatestJobRuns> {
    let scope = if job.leased { None } else { machine };
    match persist
        .web()
        .latest_scheduled_job_runs(job.name, scope)
        .await
    {
        Ok(runs) => Some(runs),
        Err(e) => {
            tracing::warn!(job = job.name, error = %e, "scheduled_jobs: could not read the run history — starting with none");
            None
        }
    }
}

pub(super) async fn save(persist: &PersistHandle, job: &Job, outcome: &Outcome<'_>) {
    if let Err(e) = persist
        .web()
        .record_scheduled_job_run(&row(job, outcome))
        .await
    {
        tracing::warn!(job = job.name, error = %e, "scheduled_jobs: could not record the run in the database — it stays in memory only");
    }
}
