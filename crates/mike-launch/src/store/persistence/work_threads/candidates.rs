use chrono::{DateTime, Utc};

use super::WorkThreadError;

#[derive(Debug, Clone, PartialEq)]
pub struct CandidateThread {
    pub thread_id: i64,
    pub project_id: i64,
    pub query_text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewThreadCandidate {
    pub reference: String,
    pub kind: String,
    pub score: f32,
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn threads_due_for_card_candidates(
        self,
        window_start: DateTime<Utc>,
    ) -> Result<Vec<CandidateThread>, WorkThreadError> {
        let rows = crate::metrics::time_db("threads_due_for_card_candidates", async move {
            sqlx::query_as!(
                CandidateThread,
                r#"WITH open AS (
                       SELECT t.id, t.project_id,
                              GREATEST(t.updated_at, s.updated_at) AS text_changed_at,
                              concat_ws(E'\n', NULLIF(btrim(t.title), ''), NULLIF(btrim(t.goal), ''),
                                        NULLIF(btrim(s.tldr), ''), NULLIF(btrim(s.where_we_are), ''),
                                        NULLIF(btrim(s.next_up), '')) AS query_text
                         FROM work.threads t
                         LEFT JOIN work.thread_state s ON s.thread_id = t.id
                        WHERE t.status IS NULL
                           OR t.status NOT IN ('done', 'complete', 'parked', 'superseded')
                   ), checked AS (
                       SELECT c.thread_id, max(c.built_at) AS built_at
                         FROM work.thread_card_candidates c
                        GROUP BY c.thread_id
                   )
                   SELECT o.id AS "thread_id!", o.project_id AS "project_id!",
                          o.query_text AS "query_text!"
                     FROM open o
                     LEFT JOIN checked b ON b.thread_id = o.id
                    WHERE o.query_text <> ''
                      AND CASE
                            WHEN b.built_at IS NOT NULL AND o.text_changed_at > b.built_at THEN true
                            WHEN b.built_at IS NULL OR b.built_at < $1 THEN
                                EXISTS (SELECT 1 FROM memory.lessons k
                                         WHERE k.project_id = o.project_id AND k.archived_at IS NULL
                                           AND k.created_at > COALESCE(b.built_at, '-infinity'))
                                OR EXISTS (SELECT 1 FROM memory.operator_turns k
                                            WHERE k.project_id = o.project_id
                                              AND (k.opened_by IS NULL OR k.opened_by = 'operator')
                                              AND k.created_at > COALESCE(b.built_at, '-infinity'))
                                OR EXISTS (SELECT 1 FROM work.thread_log k
                                             JOIN work.threads w ON w.id = k.thread_id
                                            WHERE w.project_id = o.project_id AND k.thread_id <> o.id
                                              AND k.created_at > COALESCE(b.built_at, '-infinity'))
                                OR EXISTS (SELECT 1 FROM workspace.session_notes k
                                             JOIN sessions.sessions n ON n.id = k.session_id
                                            WHERE n.project_id = o.project_id
                                              AND k.created_at > COALESCE(b.built_at, '-infinity'))
                            ELSE false
                          END
                    ORDER BY o.id"#,
                window_start,
            )
            .fetch_all(self.pool())
            .await
        })
        .await?;
        Ok(rows)
    }

    pub async fn thread_entry_ids_of(
        self,
        thread_id: i64,
        entry_ids: &[i64],
    ) -> Result<Vec<i64>, WorkThreadError> {
        let ids = crate::metrics::time_db("thread_entry_ids_of", async move {
            sqlx::query_scalar!(
                r#"SELECT l.id AS "id!"
                     FROM work.thread_log l
                    WHERE l.thread_id = $1 AND l.id = ANY($2)"#,
                thread_id,
                entry_ids,
            )
            .fetch_all(self.pool())
            .await
        })
        .await?;
        Ok(ids)
    }

    pub async fn replace_thread_card_candidates(
        self,
        thread_id: i64,
        candidates: &[NewThreadCandidate],
    ) -> Result<usize, WorkThreadError> {
        let references: Vec<String> = candidates.iter().map(|c| c.reference.clone()).collect();
        let kinds: Vec<String> = candidates.iter().map(|c| c.kind.clone()).collect();
        let scores: Vec<f32> = candidates.iter().map(|c| c.score).collect();
        let mut tx = self.pool().begin().await?;
        let stored = sqlx::query!(
            r#"SELECT ref AS "reference!", kind AS "kind!"
                 FROM work.thread_card_candidates WHERE thread_id = $1 ORDER BY rank"#,
            thread_id
        )
        .fetch_all(&mut *tx)
        .await?;
        let same = stored.len() == candidates.len()
            && stored
                .iter()
                .zip(candidates)
                .all(|(s, c)| s.reference == c.reference && s.kind == c.kind);
        if same {
            sqlx::query!(
                "UPDATE work.thread_card_candidates SET built_at = now() WHERE thread_id = $1",
                thread_id
            )
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            return Ok(0);
        }
        sqlx::query!(
            "DELETE FROM work.thread_card_candidates WHERE thread_id = $1",
            thread_id
        )
        .execute(&mut *tx)
        .await?;
        let written = sqlx::query!(
            r#"INSERT INTO work.thread_card_candidates (thread_id, rank, ref, kind, score)
               SELECT $1::bigint, u.ord::int, u.ref, u.kind, u.score
                 FROM UNNEST($2::text[], $3::text[], $4::real[])
                      WITH ORDINALITY AS u(ref, kind, score, ord)"#,
            thread_id,
            &references,
            &kinds,
            &scores,
        )
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;
        Ok(usize::try_from(written).unwrap_or(usize::MAX))
    }
}
