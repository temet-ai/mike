use sqlx::{Postgres, Transaction};

use super::super::PersistError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SplitConflictPolicy {
    #[default]
    Refuse,
    Drop,
    ResplitUnusedDev,
}

pub(super) async fn used_dev_turns(
    tx: &mut Transaction<'_, Postgres>,
    conflicts: &[i64],
) -> Result<Vec<i64>, PersistError> {
    Ok(sqlx::query_scalar!(
        r#"SELECT t.id AS "id!"
             FROM unnest($1::bigint[]) AS t(id)
            WHERE EXISTS (SELECT 1 FROM eval.run_items ri WHERE ri.operator_turn_id = t.id)
               OR EXISTS (SELECT 1 FROM eval.labels l
                            JOIN eval.session_end_run_items si ON si.label_id = l.id
                           WHERE l.operator_turn_id = t.id)
            ORDER BY t.id"#,
        conflicts,
    )
    .fetch_all(&mut **tx)
    .await?)
}

pub(super) async fn resplit_unused_dev(
    tx: &mut Transaction<'_, Postgres>,
    batch_id: i64,
    conflicts: &[i64],
) -> Result<(), PersistError> {
    sqlx::query!(
        "WITH moved_labels AS (
            UPDATE eval.labels SET split = 'holdout'
             WHERE operator_turn_id = ANY($1) AND split = 'dev'
            RETURNING id
        )
        UPDATE eval.turn_splits
           SET split = 'holdout', split_reason = 'holdout_resplit_unused_dev', assigned_at = now()
         WHERE operator_turn_id = ANY($1) AND split = 'dev'",
        conflicts,
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "UPDATE eval.import_batches
            SET manifest = manifest || jsonb_build_object('resplit_unused_dev', $2::jsonb)
          WHERE id = $1",
        batch_id,
        serde_json::json!(conflicts),
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}
