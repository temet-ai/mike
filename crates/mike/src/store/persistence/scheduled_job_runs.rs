use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::{PersistError, PersistHandle};

#[cfg(test)]
#[path = "scheduled_job_runs_db_tests.rs"]
mod scheduled_job_runs_db_tests;

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub(crate) struct JobRunRow {
    pub(crate) job_name: String,
    pub(crate) machine_uuid: Option<Uuid>,
    pub(crate) trigger: String,
    pub(crate) started_at: DateTime<Utc>,
    pub(crate) finished_at: DateTime<Utc>,
    pub(crate) duration_ms: i64,
    pub(crate) result_kind: String,
    pub(crate) exit_code: Option<i32>,
    pub(crate) result_detail: Option<String>,
    pub(crate) output_tail: String,
    pub(crate) consecutive_failures: i32,
    pub(crate) retry_after: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct LatestJobRuns {
    pub(crate) latest: Option<JobRunRow>,
    pub(crate) last_scheduled_start: Option<DateTime<Utc>>,
}

impl PersistHandle {
    pub(crate) async fn record_scheduled_job_run(
        &self,
        run: &JobRunRow,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("record_scheduled_job_run", async move {
            sqlx::query!(
                r#"INSERT INTO jobs.runs
                       (job_name, machine_uuid, trigger, started_at, finished_at, duration_ms,
                        result_kind, exit_code, result_detail, output_tail,
                        consecutive_failures, retry_after)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)"#,
                run.job_name,
                run.machine_uuid,
                run.trigger,
                run.started_at,
                run.finished_at,
                run.duration_ms,
                run.result_kind,
                run.exit_code,
                run.result_detail,
                run.output_tail,
                run.consecutive_failures,
                run.retry_after,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn latest_scheduled_job_runs(
        &self,
        job_name: &str,
        machine: Option<Uuid>,
    ) -> Result<LatestJobRuns, PersistError> {
        crate::metrics::time_db("latest_scheduled_job_runs", async move {
            let latest = self.latest_job_run(job_name, machine).await?;
            let last_scheduled_start = self.last_scheduled_job_start(job_name, machine).await?;
            Ok(LatestJobRuns {
                latest,
                last_scheduled_start,
            })
        })
        .await
    }

    async fn latest_job_run(
        &self,
        job_name: &str,
        machine: Option<Uuid>,
    ) -> Result<Option<JobRunRow>, PersistError> {
        let latest =
            match machine {
                Some(machine) => sqlx::query_as!(
                    JobRunRow,
                    r#"SELECT job_name, machine_uuid, trigger, started_at, finished_at, duration_ms,
                              result_kind, exit_code, result_detail, output_tail,
                              consecutive_failures, retry_after
                         FROM jobs.runs
                        WHERE job_name = $1
                          AND machine_uuid = $2
                        ORDER BY started_at DESC
                        LIMIT 1"#,
                    job_name,
                    machine,
                )
                .fetch_optional(&self.pool)
                .await?,
                None => sqlx::query_as!(
                    JobRunRow,
                    r#"SELECT job_name, machine_uuid, trigger, started_at, finished_at, duration_ms,
                              result_kind, exit_code, result_detail, output_tail,
                              consecutive_failures, retry_after
                         FROM jobs.runs
                        WHERE job_name = $1
                        ORDER BY started_at DESC
                        LIMIT 1"#,
                    job_name,
                )
                .fetch_optional(&self.pool)
                .await?,
            };
        Ok(latest)
    }

    async fn last_scheduled_job_start(
        &self,
        job_name: &str,
        machine: Option<Uuid>,
    ) -> Result<Option<DateTime<Utc>>, PersistError> {
        let last = match machine {
            Some(machine) => {
                sqlx::query_scalar!(
                    r#"SELECT max(started_at) AS "last_scheduled_start"
                         FROM jobs.runs
                        WHERE job_name = $1
                          AND trigger = 'schedule'
                          AND machine_uuid = $2"#,
                    job_name,
                    machine,
                )
                .fetch_one(&self.pool)
                .await?
            }
            None => {
                sqlx::query_scalar!(
                    r#"SELECT max(started_at) AS "last_scheduled_start"
                         FROM jobs.runs
                        WHERE job_name = $1
                          AND trigger = 'schedule'"#,
                    job_name,
                )
                .fetch_one(&self.pool)
                .await?
            }
        };
        Ok(last)
    }
}
