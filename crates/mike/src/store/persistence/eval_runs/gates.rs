use chrono::{DateTime, Utc};

use super::super::{PersistError, PersistHandle};
use super::{EvalGateRow, NewEvalGate};

struct CountedGate {
    id: i64,
    created_at: DateTime<Utc>,
    candidate_run_id: i64,
    baseline_run_id: Option<i64>,
    noise_run_id: Option<i64>,
    holdout_batch: Option<String>,
    verdict: String,
    floors: serde_json::Value,
    report: serde_json::Value,
    override_reason: Option<String>,
    total: i64,
}

fn split_total(rows: Vec<CountedGate>) -> (Vec<EvalGateRow>, i64) {
    let total = rows.first().map_or(0, |r| r.total);
    let gates = rows
        .into_iter()
        .map(|r| EvalGateRow {
            id: r.id,
            created_at: r.created_at,
            candidate_run_id: r.candidate_run_id,
            baseline_run_id: r.baseline_run_id,
            noise_run_id: r.noise_run_id,
            holdout_batch: r.holdout_batch,
            verdict: r.verdict,
            floors: r.floors,
            report: r.report,
            override_reason: r.override_reason,
        })
        .collect();
    (gates, total)
}

impl PersistHandle {
    pub async fn insert_eval_gate(&self, gate: &NewEvalGate<'_>) -> Result<i64, PersistError> {
        crate::metrics::time_db("insert_eval_gate", async move {
            Ok(sqlx::query_scalar!(
                r#"INSERT INTO eval.gates (candidate_run_id, baseline_run_id, noise_run_id,
                       holdout_batch, verdict, floors, report, override_reason)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                   RETURNING id AS "id!""#,
                gate.candidate_run_id,
                gate.baseline_run_id,
                gate.noise_run_id,
                gate.holdout_batch,
                gate.verdict,
                &gate.floors,
                &gate.report,
                gate.override_reason,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn count_holdout_gates(&self, holdout_batch: &str) -> Result<i64, PersistError> {
        crate::metrics::time_db("count_holdout_gates", async move {
            Ok(sqlx::query_scalar!(
                r#"SELECT count(DISTINCT r.bundle_id) AS "count!"
                     FROM eval.gates g
                     JOIN eval.runs r ON r.id = g.candidate_run_id
                    WHERE g.holdout_batch = $1
                      AND g.floors ->> 'rule' = 'promotion'"#,
                holdout_batch,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn get_eval_gate(&self, id: i64) -> Result<Option<EvalGateRow>, PersistError> {
        crate::metrics::time_db("get_eval_gate", async move {
            Ok(sqlx::query_as!(
                EvalGateRow,
                "SELECT id, created_at, candidate_run_id, baseline_run_id, noise_run_id,
                        holdout_batch, verdict, floors, report, override_reason
                   FROM eval.gates WHERE id = $1",
                id,
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn list_eval_gates(
        &self,
        holdout_batch: Option<&str>,
        limit: i64,
        offset: i64,
    ) -> Result<(Vec<EvalGateRow>, i64), PersistError> {
        crate::metrics::time_db("list_eval_gates", async move {
            let rows =
                match holdout_batch {
                    Some(batch) => sqlx::query_as!(
                        CountedGate,
                        r#"SELECT id, created_at, candidate_run_id, baseline_run_id, noise_run_id,
                                  holdout_batch, verdict, floors, report, override_reason,
                                  count(*) OVER () AS "total!"
                             FROM eval.gates
                            WHERE holdout_batch = $1
                            ORDER BY created_at DESC, id DESC LIMIT $2 OFFSET $3"#,
                        batch,
                        limit,
                        offset,
                    )
                    .fetch_all(&self.pool)
                    .await?,
                    None => sqlx::query_as!(
                        CountedGate,
                        r#"SELECT id, created_at, candidate_run_id, baseline_run_id, noise_run_id,
                                  holdout_batch, verdict, floors, report, override_reason,
                                  count(*) OVER () AS "total!"
                             FROM eval.gates
                            ORDER BY created_at DESC, id DESC LIMIT $1 OFFSET $2"#,
                        limit,
                        offset,
                    )
                    .fetch_all(&self.pool)
                    .await?,
                };
            Ok(split_total(rows))
        })
        .await
    }
}
