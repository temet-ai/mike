use super::PersistHandle;
use super::error::PersistError;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompactionSurvey {
    pub candidate_rows: i64,
    pub sessions_affected: i64,
    pub reclaimable_bytes: i64,
    pub refused_no_envelope_rows: i64,
    pub refused_no_envelope_bytes: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CompactionOutcome {
    pub cleared_rows: u64,
    pub reclaimed_bytes: i64,
    pub batches: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactionBatchOutcome {
    pub watermark: i64,
    pub cleared: u64,
    pub reclaimed_bytes: i64,
}

impl PersistHandle {
    pub async fn survey_request_body_compaction(
        &self,
        min_idle_days: i32,
    ) -> Result<CompactionSurvey, PersistError> {
        let row = sqlx::query!(
            r#"SELECT
                 count(*) FILTER (WHERE r.request_envelope IS NOT NULL)
                     AS "candidate_rows!",
                 count(DISTINCT r.session_id) FILTER (WHERE r.request_envelope IS NOT NULL)
                     AS "sessions_affected!",
                 COALESCE(
                     sum(octet_length(r.request_body))
                         FILTER (WHERE r.request_envelope IS NOT NULL), 0)
                     AS "reclaimable_bytes!",
                 count(*) FILTER (WHERE r.request_envelope IS NULL)
                     AS "refused_no_envelope_rows!",
                 COALESCE(
                     sum(octet_length(r.request_body))
                         FILTER (WHERE r.request_envelope IS NULL), 0)
                     AS "refused_no_envelope_bytes!"
               FROM requests.requests r
               JOIN sessions.sessions s ON s.id = r.session_id
              WHERE r.request_body IS NOT NULL
                AND s.last_seen < now() - make_interval(days => $1)
                AND EXISTS (
                    SELECT 1
                      FROM requests.requests later
                     WHERE later.session_id = r.session_id
                       AND later.id > r.id
                )"#,
            min_idle_days,
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(CompactionSurvey {
            candidate_rows: row.candidate_rows,
            sessions_affected: row.sessions_affected,
            reclaimable_bytes: row.reclaimable_bytes,
            refused_no_envelope_rows: row.refused_no_envelope_rows,
            refused_no_envelope_bytes: row.refused_no_envelope_bytes,
        })
    }

    pub async fn compact_request_bodies(
        &self,
        min_idle_days: i32,
        batch_size: i64,
    ) -> Result<CompactionOutcome, PersistError> {
        let mut total = CompactionOutcome::default();
        let mut cursor: i64 = 0;
        loop {
            let Some(batch) = self
                .compact_request_bodies_batch(min_idle_days, batch_size, cursor)
                .await?
            else {
                return Ok(total);
            };
            total.cleared_rows += batch.cleared;
            total.reclaimed_bytes += batch.reclaimed_bytes;
            total.batches += 1;
            cursor = batch.watermark + 1;
        }
    }

    pub async fn compact_request_bodies_batch(
        &self,
        min_idle_days: i32,
        batch_size: i64,
        cursor: i64,
    ) -> Result<Option<CompactionBatchOutcome>, PersistError> {
        crate::metrics::time_db("compact_request_bodies_batch", async move {
            let mut tx = self.background_pool().begin().await?;
            sqlx::query!("SET LOCAL statement_timeout = '60s'")
                .execute(&mut *tx)
                .await?;

            let row = sqlx::query!(
                r#"WITH victims AS (
                       SELECT r.id, octet_length(r.request_body) AS bytes
                         FROM requests.requests r
                         JOIN sessions.sessions s ON s.id = r.session_id
                        WHERE r.id >= $1
                          AND r.request_body IS NOT NULL
                          AND r.request_envelope IS NOT NULL
                          AND s.last_seen < now() - make_interval(days => $2)
                          AND EXISTS (
                              SELECT 1
                                FROM requests.requests later
                               WHERE later.session_id = r.session_id
                                 AND later.id > r.id
                          )
                        ORDER BY r.id ASC
                        LIMIT $3
                   ),
                   cleared AS (
                       UPDATE requests.requests r
                          SET request_body = NULL
                         FROM victims v
                        WHERE r.id = v.id
                       RETURNING r.id
                   )
                   SELECT count(*)                          AS "cleared!",
                          COALESCE(sum(v.bytes), 0)::bigint AS "reclaimed_bytes!",
                          max(v.id)                         AS "watermark"
                     FROM victims v
                     JOIN cleared c ON c.id = v.id"#,
                cursor,
                min_idle_days,
                batch_size,
            )
            .fetch_one(&mut *tx)
            .await?;

            let Some(watermark) = row.watermark else {
                tx.rollback().await?;
                return Ok(None);
            };
            tx.commit().await?;

            Ok(Some(CompactionBatchOutcome {
                watermark,
                cleared: u64::try_from(row.cleared).unwrap_or_default(),
                reclaimed_bytes: row.reclaimed_bytes,
            }))
        })
        .await
    }
}

#[cfg(test)]
mod tests;
