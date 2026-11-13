use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::PersistError;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ThresholdPassRun {
    pub rollup_id: i64,
    pub first_ran_at: DateTime<Utc>,
    pub last_ran_at: DateTime<Utc>,
    pub attempts: i32,
    pub outcome: String,
    pub threshold_mode: String,
    pub rows_written: i32,
    pub applied: i32,
    pub n_by_source: serde_json::Value,
    pub detail: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ThresholdPassRunWrite {
    pub rollup_id: i64,
    pub outcome: String,
    pub threshold_mode: String,
    pub rows_written: i32,
    pub applied: i32,
    pub n_by_source: serde_json::Value,
    pub detail: serde_json::Value,
}

impl crate::store::persistence::steerer_repo::SteererDb<'_> {
    pub async fn record_threshold_pass_run(
        self,
        run: &ThresholdPassRunWrite,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("record_threshold_pass_run", async move {
            sqlx::query!(
                "INSERT INTO steering.threshold_pass_runs
                        (rollup_id, outcome, threshold_mode, rows_written, applied, n_by_source, detail)
                 VALUES ($1, $2, $3, $4, $5, $6, $7)
                 ON CONFLICT (rollup_id) DO UPDATE
                    SET last_ran_at = now(),
                        attempts = steering.threshold_pass_runs.attempts + 1,
                        outcome = EXCLUDED.outcome,
                        threshold_mode = EXCLUDED.threshold_mode,
                        rows_written = EXCLUDED.rows_written,
                        applied = EXCLUDED.applied,
                        n_by_source = EXCLUDED.n_by_source,
                        detail = EXCLUDED.detail",
                run.rollup_id,
                run.outcome,
                run.threshold_mode,
                run.rows_written,
                run.applied,
                run.n_by_source,
                run.detail
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn recent_threshold_pass_runs(
        self,
        limit: i64,
    ) -> Result<Vec<ThresholdPassRun>, PersistError> {
        crate::metrics::time_db("recent_threshold_pass_runs", async move {
            Ok(sqlx::query_as!(
                ThresholdPassRun,
                "SELECT rollup_id, first_ran_at, last_ran_at, attempts, outcome, threshold_mode,
                        rows_written, applied, n_by_source, detail
                   FROM steering.threshold_pass_runs
                  ORDER BY rollup_id DESC
                  LIMIT $1",
                limit
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}
