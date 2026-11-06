use chrono::{DateTime, Utc};

use super::super::{PersistError, PersistHandle};
use crate::eval::labels::import::fleet_mirror::{ImportPlan, SOURCE_KEY};

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ResolvedTurn {
    pub turn_id: i64,
    pub session_id: i64,
    pub session_uuid: String,
    pub closed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportOutcome {
    Written {
        batch_id: i64,
        labels: u64,
        splits: u64,
        exclusions: u64,
    },
    AlreadyImported {
        batch_id: i64,
        imported_at: DateTime<Utc>,
    },
    TurnInHoldout(Vec<i64>),
}

impl PersistHandle {
    pub async fn resolve_label_turns(
        &self,
        turn_ids: &[i64],
    ) -> Result<Vec<ResolvedTurn>, PersistError> {
        crate::metrics::time_db("resolve_label_turns", async move {
            let rows = sqlx::query_as!(
                ResolvedTurn,
                r#"SELECT t.id AS "turn_id!", t.session_id AS "session_id!",
                          s.session_uuid::text AS "session_uuid!",
                          (t.closed_at IS NOT NULL) AS "closed!"
                     FROM unnest($1::bigint[]) AS want(id)
                     JOIN memory.operator_turns t ON t.id = want.id
                     JOIN sessions.sessions s ON s.id = t.session_id"#,
                turn_ids,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn import_fleet_mirror(
        &self,
        plan: &ImportPlan,
        resolved: &[ResolvedTurn],
        imported_by: &str,
    ) -> Result<ImportOutcome, PersistError> {
        let by_turn: std::collections::HashMap<i64, &ResolvedTurn> =
            resolved.iter().map(|r| (r.turn_id, r)).collect();
        let rows: Vec<serde_json::Value> = plan
            .rows
            .iter()
            .filter_map(|row| {
                let turn = by_turn.get(&row.subject_id)?;
                let mut value = serde_json::to_value(row).ok()?;
                let map = value.as_object_mut()?;
                map.insert("session_id".to_owned(), serde_json::json!(turn.session_id));
                map.insert(
                    "session_uuid".to_owned(),
                    serde_json::json!(turn.session_uuid),
                );
                Some(value)
            })
            .collect();
        let turns: Vec<i64> = plan.turns.iter().copied().collect();
        let exclusions = serde_json::to_value(&plan.exclusions)?;
        let rows = serde_json::Value::Array(rows);
        crate::metrics::time_db("import_fleet_mirror", async move {
            let mut tx = self.pool.begin().await?;
            let batch = sqlx::query!(
                r#"INSERT INTO eval.import_batches (source_key, manifest, counts, imported_by)
                   VALUES ($1, $2, $3, $4)
                   ON CONFLICT (source_key) DO SELECT
                   RETURNING id AS "id!", imported_at AS "imported_at!",
                             (old.id IS NULL) AS "inserted!""#,
                SOURCE_KEY,
                &plan.manifest,
                &plan.counts,
                imported_by,
            )
            .fetch_one(&mut *tx)
            .await?;
            if !batch.inserted {
                tx.rollback().await?;
                return Ok(ImportOutcome::AlreadyImported {
                    batch_id: batch.id,
                    imported_at: batch.imported_at,
                });
            }
            let batch_id = batch.id;
            let splits = sqlx::query!(
                "INSERT INTO eval.turn_splits (operator_turn_id, split, split_reason)
                 SELECT id, 'dev', 'seed_import' FROM unnest($1::bigint[]) AS t(id)
                 ON CONFLICT (operator_turn_id) DO NOTHING",
                &turns,
            )
            .execute(&mut *tx)
            .await?
            .rows_affected();
            let holdout = sqlx::query_scalar!(
                "SELECT operator_turn_id FROM eval.turn_splits
                  WHERE operator_turn_id = ANY($1) AND split = 'holdout'
                  ORDER BY operator_turn_id",
                &turns,
            )
            .fetch_all(&mut *tx)
            .await?;
            if !holdout.is_empty() {
                tx.rollback().await?;
                return Ok(ImportOutcome::TurnInHoldout(holdout));
            }
            let labels = sqlx::query_file!(
                "src/store/persistence/eval_labels/seed_labels.sql",
                &rows,
                batch_id,
            )
            .execute(&mut *tx)
            .await?
            .rows_affected();
            let excluded = sqlx::query!(
                "INSERT INTO eval.population_exclusions
                        (session_id, operator_turn_id, reason, import_batch_id)
                 SELECT s.id, x.turn_id, x.reason, $2
                   FROM jsonb_to_recordset($1::jsonb) AS x(session_uuid text, turn_id bigint, reason text)
                   JOIN sessions.sessions s ON s.session_uuid = x.session_uuid::uuid",
                &exclusions,
                batch_id,
            )
            .execute(&mut *tx)
            .await?
            .rows_affected();
            tx.commit().await?;
            Ok(ImportOutcome::Written {
                batch_id,
                labels,
                splits,
                exclusions: excluded,
            })
        })
        .await
    }
}
