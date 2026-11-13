use chrono::{DateTime, Utc};

use sqlx::{Postgres, Transaction};

use super::super::PersistError;
use super::holdout_resplit::{SplitConflictPolicy, resplit_unused_dev, used_dev_turns};
use super::import::ResolvedTurn;
use crate::eval::labels::import::holdout::HoldoutPlan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HoldoutOutcome {
    Written {
        batch_id: i64,
        labels: u64,
        splits: u64,
        dropped: Vec<i64>,
    },
    AlreadyImported {
        batch_id: i64,
        imported_at: DateTime<Utc>,
    },
    DevSplitConflict(Vec<i64>),
    DevLabelsInUse(Vec<i64>),
}

enum ConflictResolution {
    Proceed(Vec<i64>),
    Abort(HoldoutOutcome),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WithdrawReport {
    pub labels: u64,
    pub executed: bool,
}

async fn resolve_conflicts(
    tx: &mut Transaction<'_, Postgres>,
    batch_id: i64,
    conflicts: Vec<i64>,
    policy: SplitConflictPolicy,
) -> Result<ConflictResolution, PersistError> {
    if conflicts.is_empty() {
        return Ok(ConflictResolution::Proceed(Vec::new()));
    }
    match policy {
        SplitConflictPolicy::Refuse => Ok(ConflictResolution::Abort(
            HoldoutOutcome::DevSplitConflict(conflicts),
        )),
        SplitConflictPolicy::Drop => {
            sqlx::query!(
                "UPDATE eval.import_batches
                    SET manifest = manifest || jsonb_build_object('dropped_split_conflicts', $2::jsonb)
                  WHERE id = $1",
                batch_id,
                serde_json::json!(conflicts),
            )
            .execute(&mut **tx)
            .await?;
            Ok(ConflictResolution::Proceed(conflicts))
        }
        SplitConflictPolicy::ResplitUnusedDev => {
            let used = used_dev_turns(tx, &conflicts).await?;
            if !used.is_empty() {
                return Ok(ConflictResolution::Abort(HoldoutOutcome::DevLabelsInUse(
                    used,
                )));
            }
            resplit_unused_dev(tx, batch_id, &conflicts).await?;
            Ok(ConflictResolution::Proceed(Vec::new()))
        }
    }
}

fn label_payload(
    plan: &HoldoutPlan,
    resolved: &[ResolvedTurn],
    dropped: &[i64],
) -> serde_json::Value {
    let by_turn: std::collections::HashMap<i64, &ResolvedTurn> =
        resolved.iter().map(|r| (r.turn_id, r)).collect();
    let rows: Vec<serde_json::Value> = plan
        .rows
        .iter()
        .filter(|row| !dropped.contains(&row.operator_turn_id))
        .filter_map(|row| {
            let turn = by_turn.get(&row.operator_turn_id)?;
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
    serde_json::Value::Array(rows)
}

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub async fn import_holdout(
        self,
        plan: &HoldoutPlan,
        resolved: &[ResolvedTurn],
        imported_by: &str,
        policy: SplitConflictPolicy,
    ) -> Result<HoldoutOutcome, PersistError> {
        let turns: Vec<i64> = plan.turns.iter().copied().collect();
        crate::metrics::time_db("import_holdout", async move {
            let mut tx = self.pool.begin().await?;
            let batch = sqlx::query!(
                r#"INSERT INTO eval.import_batches (source_key, manifest, counts, imported_by)
                   VALUES ($1, $2, $3, $4)
                   ON CONFLICT (source_key) DO SELECT
                   RETURNING id AS "id!", imported_at AS "imported_at!",
                             (old.id IS NULL) AS "inserted!""#,
                &plan.batch,
                &plan.manifest,
                &plan.counts,
                imported_by,
            )
            .fetch_one(&mut *tx)
            .await?;
            if !batch.inserted {
                tx.rollback().await?;
                return Ok(HoldoutOutcome::AlreadyImported {
                    batch_id: batch.id,
                    imported_at: batch.imported_at,
                });
            }
            let batch_id = batch.id;
            let splits = sqlx::query!(
                "INSERT INTO eval.turn_splits (operator_turn_id, split, split_reason)
                 SELECT id, 'holdout', 'holdout_import' FROM unnest($1::bigint[]) AS t(id)
                 ON CONFLICT (operator_turn_id) DO NOTHING",
                &turns,
            )
            .execute(&mut *tx)
            .await?
            .rows_affected();
            let conflicts = sqlx::query_scalar!(
                "SELECT operator_turn_id FROM eval.turn_splits
                  WHERE operator_turn_id = ANY($1) AND split = 'dev' ORDER BY operator_turn_id",
                &turns,
            )
            .fetch_all(&mut *tx)
            .await?;
            let dropped = match resolve_conflicts(&mut tx, batch_id, conflicts, policy).await? {
                ConflictResolution::Proceed(dropped) => dropped,
                ConflictResolution::Abort(outcome) => {
                    tx.rollback().await?;
                    return Ok(outcome);
                }
            };
            let labels = sqlx::query_file!(
                "src/store/persistence/eval_labels/holdout_labels.sql",
                label_payload(plan, resolved, &dropped),
                batch_id,
            )
            .execute(&mut *tx)
            .await?
            .rows_affected();
            tx.commit().await?;
            Ok(HoldoutOutcome::Written {
                batch_id,
                labels,
                splits,
                dropped,
            })
        })
        .await
    }

    pub async fn withdraw_label_batch(
        self,
        source_key: &str,
        reason: &str,
        withdrawn_by: &str,
        execute: bool,
    ) -> Result<WithdrawReport, PersistError> {
        crate::metrics::time_db("withdraw_label_batch", async move {
            if !execute {
                let n = sqlx::query_scalar!(
                    r#"WITH current AS (
                           SELECT l.id FROM eval.labels_as_of(now()) l
                             JOIN eval.import_batches b ON b.id = l.import_batch_id
                            WHERE b.source_key = $1
                       )
                       SELECT count(*) AS "count!" FROM current"#,
                    source_key,
                )
                .fetch_one(&self.pool)
                .await?;
                return Ok(WithdrawReport {
                    labels: u64::try_from(n).unwrap_or(0),
                    executed: false,
                });
            }
            let labels = sqlx::query_file!(
                "src/store/persistence/eval_labels/withdraw_batch.sql",
                source_key,
                reason,
                withdrawn_by,
            )
            .execute(&self.pool)
            .await?
            .rows_affected();
            Ok(WithdrawReport {
                labels,
                executed: true,
            })
        })
        .await
    }
}
