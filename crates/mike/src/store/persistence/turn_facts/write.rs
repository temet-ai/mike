use serde::Serialize;

use super::TurnFactsRow;
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct UpsertOutcome {
    pub inserted: u64,
    pub updated: u64,
    pub reclassified: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct OpenedByRow {
    pub turn_id: i64,
    pub opened_by: String,
}

impl PersistHandle {
    pub async fn upsert_turn_facts(
        &self,
        rows: &[TurnFactsRow],
    ) -> Result<UpsertOutcome, PersistError> {
        if rows.is_empty() {
            return Ok(UpsertOutcome::default());
        }
        let payload = serde_json::to_value(rows)?;
        crate::metrics::time_db("upsert_turn_facts", async move {
            let written =
                sqlx::query_file!("src/store/persistence/turn_facts/upsert.sql", &payload,)
                    .fetch_all(&self.pool)
                    .await?;
            Ok(written
                .into_iter()
                .fold(UpsertOutcome::default(), |mut out, row| {
                    if row.inserted {
                        out.inserted += 1;
                    } else {
                        out.updated += 1;
                    }
                    out.reclassified += u64::from(row.reclassified);
                    out
                }))
        })
        .await
    }

    pub async fn stamp_turn_facts_built_at(
        &self,
        turn_ids: &[i64],
        built_at: chrono::DateTime<chrono::Utc>,
    ) -> Result<u64, PersistError> {
        if turn_ids.is_empty() {
            return Ok(0);
        }
        crate::metrics::time_db("stamp_turn_facts_built_at", async move {
            let done = sqlx::query!(
                "WITH cleared AS (
                     DELETE FROM memory.turn_facts_backstop_attempts a
                      WHERE a.operator_turn_id = ANY($1)
                 )
                 UPDATE memory.turn_facts
                    SET built_at = $2
                  WHERE operator_turn_id = ANY($1)
                    AND built_at < $2",
                turn_ids,
                built_at,
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }

    pub async fn fill_operator_turn_opened_by(
        &self,
        rows: &[OpenedByRow],
    ) -> Result<u64, PersistError> {
        if rows.is_empty() {
            return Ok(0);
        }
        let payload = serde_json::to_value(rows)?;
        crate::metrics::time_db("fill_operator_turn_opened_by", async move {
            let done = sqlx::query!(
                "UPDATE memory.operator_turns t
                    SET opened_by = x.opened_by
                   FROM jsonb_to_recordset($1::jsonb) AS x(turn_id bigint, opened_by text)
                  WHERE t.id = x.turn_id
                    AND t.opened_by IS NULL",
                &payload,
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }
}
