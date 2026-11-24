use super::PersistHandle;
use super::error::PersistError;
use super::sent_content;

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
    pub refused_rows: u64,
    pub reclaimed_bytes: i64,
    pub batches: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactionBatchOutcome {
    pub watermark: i64,
    pub cleared: u64,
    pub refused: u64,
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
        ids: std::ops::RangeInclusive<i64>,
    ) -> Result<CompactionOutcome, PersistError> {
        let mut total = CompactionOutcome::default();
        let mut cursor: i64 = *ids.start();
        let last = *ids.end();
        loop {
            let Some(batch) = self
                .compact_request_bodies_batch(min_idle_days, batch_size, cursor, last)
                .await?
            else {
                return Ok(total);
            };
            total.cleared_rows += batch.cleared;
            total.refused_rows += batch.refused;
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
        last: i64,
    ) -> Result<Option<CompactionBatchOutcome>, PersistError> {
        crate::metrics::time_db("compact_request_bodies_batch", async move {
            let mut tx = self.background_pool().begin().await?;
            sqlx::query!("SET LOCAL statement_timeout = '60s'")
                .execute(&mut *tx)
                .await?;

            let candidates =
                compaction_candidates(&mut tx, min_idle_days, batch_size, cursor, last).await?;
            let Some(watermark) = candidates.last().map(|c| c.id) else {
                tx.rollback().await?;
                return Ok(None);
            };
            let clearable = sent_content::clearable_ids(&mut tx, &candidates).await?;
            let reclaimed_bytes = candidates
                .iter()
                .filter(|c| clearable.contains(&c.id))
                .map(|c| c.request_body.len() as i64)
                .sum();
            let cleared = sqlx::query!(
                "UPDATE requests.requests
                    SET request_body = NULL
                  WHERE id = ANY($1)
                    AND request_body IS NOT NULL",
                &clearable,
            )
            .execute(&mut *tx)
            .await?
            .rows_affected();
            tx.commit().await?;

            Ok(Some(CompactionBatchOutcome {
                watermark,
                cleared,
                refused: (candidates.len() - clearable.len()) as u64,
                reclaimed_bytes,
            }))
        })
        .await
    }
}

async fn compaction_candidates(
    conn: &mut sqlx::PgConnection,
    min_idle_days: i32,
    batch_size: i64,
    cursor: i64,
    last: i64,
) -> Result<Vec<sent_content::ClearCandidate>, PersistError> {
    let rows = sqlx::query!(
        r#"SELECT r.id,
                  r.request_body AS "request_body!",
                  (r.filtered_request_body IS NOT NULL) AS "has_filtered_body!"
             FROM requests.requests r
             JOIN sessions.sessions s ON s.id = r.session_id
            WHERE r.id >= $1
              AND r.id <= $4
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
            LIMIT $3"#,
        cursor,
        min_idle_days,
        batch_size,
        last,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| sent_content::ClearCandidate {
            id: row.id,
            request_body: row.request_body,
            has_filtered_body: row.has_filtered_body,
        })
        .collect())
}

#[cfg(test)]
mod tests;
