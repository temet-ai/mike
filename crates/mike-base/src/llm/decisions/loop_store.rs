use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

use crate::store::persistence::PersistError;

#[derive(Debug, Clone, Default, PartialEq, Serialize, sqlx::FromRow)]
pub struct PointCounts {
    pub point: String,
    pub calls: i64,
    pub applied_on: i64,
    pub fail_open: i64,
    pub schema_errors: i64,
    pub refused: i64,
    pub question_refusals: i64,
    pub cost_usd: f64,
    pub latency_p50_ms: Option<f64>,
    pub latency_p95_ms: Option<f64>,
    pub labelled: i64,
    pub labelled_last_7d: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct LabelledDecision {
    pub point: String,
    pub score: Option<f64>,
    pub applied: Option<String>,
    pub label: String,
}

impl super::DecisionStore<'_> {
    pub async fn decision_point_counts(
        &self,
        since: DateTime<Utc>,
    ) -> Result<Vec<PointCounts>, PersistError> {
        crate::metrics::time_db("decision_point_counts", async move {
            Ok(
                sqlx::query_file_as!(PointCounts, "src/llm/decisions/sql/point_counts.sql", since)
                    .fetch_all(self.0.pool())
                    .await?,
            )
        })
        .await
    }

    pub async fn labelled_decisions(
        &self,
        since: DateTime<Utc>,
    ) -> Result<Vec<LabelledDecision>, PersistError> {
        crate::metrics::time_db("labelled_decisions", async move {
            Ok(sqlx::query_file_as!(
                LabelledDecision,
                "src/llm/decisions/sql/labelled.sql",
                since
            )
            .fetch_all(self.0.pool())
            .await?)
        })
        .await
    }

    pub async fn sync_evaluation_outcomes(&self, point: &str) -> Result<u64, PersistError> {
        crate::metrics::time_db("sync_evaluation_outcomes", async move {
            Ok(
                sqlx::query_file!("src/llm/decisions/sql/sync_guard_outcomes.sql", point)
                    .execute(self.0.pool())
                    .await?
                    .rows_affected(),
            )
        })
        .await
    }

    pub async fn last_decision_loop_at(&self) -> Result<Option<DateTime<Utc>>, PersistError> {
        crate::metrics::time_db("last_decision_loop_at", async move {
            Ok(sqlx::query_scalar::<_, Option<DateTime<Utc>>>(
                "SELECT max(finished_at) FROM decisions.loop_runs",
            )
            .fetch_one(self.0.pool())
            .await?)
        })
        .await
    }

    pub async fn insert_decision_loop_run(
        &self,
        started_at: DateTime<Utc>,
        report: &Value,
    ) -> Result<i64, PersistError> {
        let machine = self.0.llm_machine_uuid().await;
        crate::metrics::time_db("insert_decision_loop_run", async move {
            Ok(sqlx::query_scalar::<_, i64>(
                "INSERT INTO decisions.loop_runs (started_at, finished_at, machine_uuid, report)
                 VALUES ($1, now(), $2, $3) RETURNING id",
            )
            .bind(started_at)
            .bind(machine)
            .bind(report)
            .fetch_one(self.0.pool())
            .await?)
        })
        .await
    }
}
