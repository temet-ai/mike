use sqlx::{Postgres, QueryBuilder};

use super::super::{PersistError, PersistHandle};
use super::{EvalRunRow, NewEvalRun, RunsFilter};

const LIST_SELECT: &str =
    "SELECT r.id, r.created_at, r.finished_at, r.purpose, r.status, r.bundle_id,
        r.rubric_version, r.input_builder_version, r.slices, r.model, r.split, r.tiers,
        r.holdout_batch, r.labels_as_of, r.label_count, r.label_ids_hash, r.repeat_of_run_id,
        r.max_usd, r.cost_usd, r.items_judged, r.items_refused, r.items_failed, r.report, r.argv,
        count(*) OVER () AS total
   FROM eval.runs r";

fn list_runs_query(f: &RunsFilter) -> QueryBuilder<Postgres> {
    let mut qb = QueryBuilder::new(LIST_SELECT);
    if f.role.is_some() {
        qb.push(" JOIN eval.bundles b ON b.id = r.bundle_id");
    }
    if f.role.is_some() || f.status.is_some() || f.split.is_some() || f.purpose.is_some() {
        qb.push(" WHERE ");
        let mut conditions = qb.separated(" AND ");
        if let Some(role) = &f.role {
            conditions.push("b.role = ").push_bind_unseparated(role);
        }
        if let Some(status) = &f.status {
            conditions.push("r.status = ").push_bind_unseparated(status);
        }
        if let Some(split) = &f.split {
            conditions.push("r.split = ").push_bind_unseparated(split);
        }
        if let Some(purpose) = &f.purpose {
            conditions
                .push("r.purpose = ")
                .push_bind_unseparated(purpose);
        }
    }
    qb.push(" ORDER BY r.created_at DESC, r.id DESC LIMIT ")
        .push_bind(f.limit)
        .push(" OFFSET ")
        .push_bind(f.offset);
    qb
}

impl PersistHandle {
    pub async fn create_eval_run(&self, run: &NewEvalRun<'_>) -> Result<i64, PersistError> {
        crate::metrics::time_db("create_eval_run", async move {
            Ok(sqlx::query_scalar!(
                r#"INSERT INTO eval.runs (purpose, bundle_id, rubric_version, input_builder_version,
                       model, split, tiers, holdout_batch, labels_as_of, label_count, label_ids_hash,
                       repeat_of_run_id, max_usd, argv)
                   VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)
                   RETURNING id AS "id!""#,
                run.purpose,
                run.bundle_id,
                run.rubric_version,
                run.input_builder_version,
                run.model,
                run.split,
                run.tiers,
                run.holdout_batch,
                run.labels_as_of,
                run.label_count,
                run.label_ids_hash,
                run.repeat_of_run_id,
                run.max_usd,
                &run.argv,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn refresh_eval_run(
        &self,
        run_id: i64,
        status: &str,
        report: Option<&serde_json::Value>,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("refresh_eval_run", async move {
            sqlx::query!(
                "WITH counts AS (
                    SELECT count(*) FILTER (WHERE i.outcome = 'judged') AS judged,
                           count(*) FILTER (WHERE i.outcome = 'refused') AS refused,
                           count(*) FILTER (WHERE i.outcome = 'failed') AS failed,
                           coalesce(sum(v.cost_usd), 0) AS cost
                      FROM eval.run_items i
                      LEFT JOIN memory.turn_verdicts v ON v.id = i.turn_verdict_id
                     WHERE i.run_id = $1
                )
                UPDATE eval.runs r
                   SET status = $2,
                       items_judged = counts.judged::int,
                       items_refused = counts.refused::int,
                       items_failed = counts.failed::int,
                       cost_usd = counts.cost,
                       report = coalesce($3, r.report),
                       finished_at = CASE WHEN $2 IN ('complete', 'failed', 'cancelled')
                                          THEN now() ELSE r.finished_at END
                  FROM counts
                 WHERE r.id = $1",
                run_id,
                status,
                report,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn get_eval_run(&self, id: i64) -> Result<Option<EvalRunRow>, PersistError> {
        crate::metrics::time_db("get_eval_run", async move {
            Ok(sqlx::query_as!(
                EvalRunRow,
                "SELECT id, created_at, finished_at, purpose, status, bundle_id, rubric_version,
                        input_builder_version, slices, model, split, tiers, holdout_batch,
                        labels_as_of, label_count, label_ids_hash, repeat_of_run_id, max_usd,
                        cost_usd, items_judged, items_refused, items_failed, report, argv
                   FROM eval.runs WHERE id = $1",
                id,
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn list_eval_runs(
        &self,
        f: &RunsFilter,
    ) -> Result<(Vec<EvalRunRow>, i64), PersistError> {
        crate::metrics::time_db("list_eval_runs", async move {
            let rows = list_runs_query(f)
                .build_query_as::<Counted>()
                .fetch_all(&self.pool)
                .await?;
            let total = rows.first().map_or(0, |r| r.total);
            Ok((rows.into_iter().map(|r| r.row).collect(), total))
        })
        .await
    }

    pub async fn eval_spend_today_usd(&self) -> Result<f64, PersistError> {
        crate::metrics::time_db("eval_spend_today_usd", async move {
            Ok(sqlx::query_scalar!(
                r#"SELECT coalesce(sum(cost_usd), 0)::float8 AS "spend!" FROM llm.calls
                    WHERE role = 'eval' AND created_at >= date_trunc('day', now() AT TIME ZONE 'utc')"#,
            )
            .fetch_one(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn find_resolver_bundle(
        &self,
        role: &str,
        components: &serde_json::Value,
    ) -> Result<Option<i64>, PersistError> {
        crate::metrics::time_db("find_resolver_bundle", async move {
            Ok(sqlx::query_scalar!(
                "SELECT id FROM eval.bundles
                  WHERE role = $1 AND subject_key = '' AND origin = 'resolver'
                    AND superseded_at IS NULL AND components = $2
                  ORDER BY id DESC LIMIT 1",
                role,
                components,
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }
}

#[derive(sqlx::FromRow)]
struct Counted {
    #[sqlx(flatten)]
    row: EvalRunRow,
    total: i64,
}
