use std::sync::atomic::{AtomicBool, Ordering};

use super::PendingEmbeddingRow;
use crate::store::persistence::{PersistError, PersistHandle};

/// Whether this process may have left a closed turn unembedded since the embed
/// drain last looked. A turn closed through a request's terminal write is embedded
/// right away; this is set when that did not happen (no embedding service, the
/// semaphore was full, the call failed) and when a turn closes another way (an
/// aborted request, a session end, the idle reap). Starts true so the first drain
/// after a start looks.
struct TurnEmbedsOwed(AtomicBool);

impl Default for TurnEmbedsOwed {
    fn default() -> Self {
        Self(AtomicBool::new(true))
    }
}

impl PersistHandle {
    pub fn note_turn_embed_owed(&self) {
        self.memo::<TurnEmbedsOwed>()
            .0
            .store(true, Ordering::Release);
    }

    /// Clears the flag and says whether it was set.
    pub fn take_turn_embed_owed(&self) -> bool {
        self.memo::<TurnEmbedsOwed>()
            .0
            .swap(false, Ordering::AcqRel)
    }

    pub async fn get_turn_for_embedding(
        &self,
        turn_id: i64,
    ) -> Result<Option<PendingEmbeddingRow>, PersistError> {
        crate::metrics::time_db("get_turn_for_embedding", async move {
            let row = sqlx::query_as!(
                PendingEmbeddingRow,
                r#"SELECT id AS turn_id,
                          COALESCE(user_prompt, '')          AS "user_prompt!",
                          COALESCE(final_assistant_text, '') AS "final_assistant_text!"
                     FROM memory.operator_turns
                    WHERE id          = $1
                      AND closed_at   IS NOT NULL
                      AND embedded_at IS NULL
                      AND (opened_by IS NULL OR opened_by = 'operator')"#,
                turn_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    pub async fn fetch_unembedded_turn_ids(&self, batch: i64) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("fetch_unembedded_turn_ids", async move {
            let ids = sqlx::query_scalar!(
                "SELECT id
                   FROM memory.operator_turns
                  WHERE closed_at   IS NOT NULL
                    AND embedded_at IS NULL
                    AND (opened_by IS NULL OR opened_by = 'operator')
                  ORDER BY closed_at ASC
                  LIMIT $1",
                batch,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(ids)
        })
        .await
    }

    pub async fn mark_turns_embedded(&self, turn_ids: &[i64]) -> Result<(), PersistError> {
        crate::metrics::time_db("mark_turns_embedded", async move {
            sqlx::query!(
                "UPDATE memory.operator_turns
                    SET embedded_at = NOW()
                  WHERE id = ANY($1)
                    AND embedded_at IS NULL",
                turn_ids,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn requeue_turns_embedded_with_other_model(
        &self,
        model: &str,
    ) -> Result<u64, PersistError> {
        crate::metrics::time_db("requeue_turns_embedded_with_other_model", async move {
            let result = sqlx::query!(
                "UPDATE memory.operator_turns t
                    SET embedded_at = NULL
                  WHERE t.embedded_at IS NOT NULL
                    AND t.closed_at IS NOT NULL
                    AND (t.opened_by IS NULL OR t.opened_by = 'operator')
                    AND EXISTS (
                        SELECT 1
                          FROM memory.operator_turn_chunks c
                         WHERE c.turn_id = t.id
                           AND c.embedding_model IS DISTINCT FROM $1
                    )",
                model,
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected())
        })
        .await
    }

    pub async fn requeue_turns_marked_embedded_without_chunks(&self) -> Result<u64, PersistError> {
        crate::metrics::time_db("requeue_turns_marked_embedded_without_chunks", async move {
            let result = sqlx::query!(
                "UPDATE memory.operator_turns t
                    SET embedded_at = NULL
                  WHERE t.embedded_at IS NOT NULL
                    AND t.closed_at IS NOT NULL
                    AND (t.opened_by IS NULL OR t.opened_by = 'operator')
                    AND (btrim(COALESCE(t.user_prompt, '')) <> ''
                         OR btrim(COALESCE(t.final_assistant_text, '')) <> '')
                    AND NOT EXISTS (
                        SELECT 1 FROM memory.operator_turn_chunks c WHERE c.turn_id = t.id
                    )"
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected())
        })
        .await
    }

    pub async fn count_pending_embedding_turns(&self) -> Result<i64, PersistError> {
        crate::metrics::time_db("count_pending_embedding_turns", async move {
            let n: i64 = sqlx::query_scalar!(
                r#"SELECT COUNT(*)::bigint AS "n!"
                     FROM memory.operator_turns
                    WHERE closed_at   IS NOT NULL
                      AND embedded_at IS NULL
                      AND (opened_by IS NULL OR opened_by = 'operator')"#
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(n)
        })
        .await
    }

    pub async fn count_open_unembedded_turns(&self) -> Result<i64, PersistError> {
        crate::metrics::time_db("count_open_unembedded_turns", async move {
            let n: i64 = sqlx::query_scalar!(
                "SELECT COUNT(*)::bigint
                   FROM memory.operator_turns
                  WHERE closed_at IS NULL"
            )
            .fetch_one(&self.pool)
            .await?
            .unwrap_or(0);
            Ok(n)
        })
        .await
    }
}

pub fn vector_to_text_literal(v: &[f32]) -> String {
    let mut s = String::with_capacity(v.len() * 12 + 2);
    s.push('[');
    for (i, x) in v.iter().enumerate() {
        if i > 0 {
            s.push(',');
        }
        s.push_str(&format!("{x}"));
    }
    s.push(']');
    s
}

#[cfg(test)]
mod tests;
