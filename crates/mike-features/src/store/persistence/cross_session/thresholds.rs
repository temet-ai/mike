use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::PersistError;

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct ThresholdRow {
    pub id: i64,
    pub steerer_id: i64,
    pub question: String,
    pub scope: String,
    pub project_id: Option<i64>,
    pub computed_at: DateTime<Utc>,
    pub rollup_id: i64,
    pub applied_threshold: f32,
    pub decision: String,
    pub detail: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ThresholdWrite {
    pub steerer_id: i64,
    pub question: String,
    pub scope: String,
    pub project_id: Option<i64>,
    pub rollup_id: i64,
    pub rollup_finished_at: DateTime<Utc>,
    pub window_start: DateTime<Utc>,
    pub window_end: DateTime<Utc>,
    pub label_basis: String,
    pub question_mode: String,
    pub threshold_mode: String,
    pub n_labelled: i32,
    pub precision_bar: f64,
    pub threshold_floor: f32,
    pub threshold_ceiling: f32,
    pub static_threshold: f32,
    pub candidate_threshold: Option<f32>,
    pub applied_threshold: f32,
    pub decision: String,
    pub detail: serde_json::Value,
}

fn col<T: Clone>(rows: &[ThresholdWrite], f: impl Fn(&ThresholdWrite) -> T) -> Vec<T> {
    rows.iter().map(f).collect()
}

impl crate::store::persistence::steerer_repo::SteererDb<'_> {
    pub async fn thresholds_in_force(
        self,
        steerer_id: i64,
        as_of: DateTime<Utc>,
    ) -> Result<Vec<ThresholdRow>, PersistError> {
        crate::metrics::time_db("thresholds_in_force", async move {
            Ok(sqlx::query_as!(
                ThresholdRow,
                "SELECT DISTINCT ON (question, scope, project_id)
                        id, steerer_id, question, scope, project_id, computed_at, rollup_id,
                        applied_threshold, decision, detail
                   FROM steering.guard_thresholds
                  WHERE steerer_id = $1 AND decision = 'applied' AND computed_at <= $2
                  ORDER BY question, scope, project_id, computed_at DESC, id DESC",
                steerer_id,
                as_of,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn threshold_history(
        self,
        steerer_id: i64,
        before_id: Option<i64>,
        limit: i64,
    ) -> Result<Vec<ThresholdRow>, PersistError> {
        crate::metrics::time_db("threshold_history", async move {
            Ok(sqlx::query_as!(
                ThresholdRow,
                "SELECT id, steerer_id, question, scope, project_id, computed_at, rollup_id,
                        applied_threshold, decision, detail
                   FROM steering.guard_thresholds
                  WHERE steerer_id = $1 AND id < $2
                  ORDER BY id DESC
                  LIMIT $3",
                steerer_id,
                before_id.unwrap_or(i64::MAX),
                limit,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn insert_threshold_rows(
        self,
        rows: &[ThresholdWrite],
    ) -> Result<Vec<i64>, PersistError> {
        if rows.is_empty() {
            return Ok(Vec::new());
        }
        crate::metrics::time_db("insert_threshold_rows", async move {
            let project_ids = col(rows, |r| r.project_id);
            let candidate_thresholds = col(rows, |r| r.candidate_threshold);
            let mut tx = self.pool.begin().await?;
            let ids = sqlx::query_file_scalar!(
                "src/store/persistence/cross_session/insert_thresholds.sql",
                &col(rows, |r| r.steerer_id),
                &col(rows, |r| r.question.clone()),
                &col(rows, |r| r.scope.clone()),
                &project_ids as &[Option<i64>],
                &col(rows, |r| r.rollup_id),
                &col(rows, |r| r.rollup_finished_at),
                &col(rows, |r| r.window_start),
                &col(rows, |r| r.window_end),
                &col(rows, |r| r.label_basis.clone()),
                &col(rows, |r| r.question_mode.clone()),
                &col(rows, |r| r.threshold_mode.clone()),
                &col(rows, |r| r.n_labelled),
                &col(rows, |r| r.precision_bar),
                &col(rows, |r| r.threshold_floor),
                &col(rows, |r| r.threshold_ceiling),
                &col(rows, |r| r.static_threshold),
                &candidate_thresholds as &[Option<f32>],
                &col(rows, |r| r.applied_threshold),
                &col(rows, |r| r.decision.clone()),
                &col(rows, |r| r.detail.clone()),
            )
            .fetch_all(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(ids)
        })
        .await
    }
}
