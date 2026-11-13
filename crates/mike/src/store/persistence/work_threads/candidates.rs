use super::WorkThreadError;

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct CandidateThread {
    pub(crate) thread_id: i64,
    pub(crate) project_id: i64,
    pub(crate) query_text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NewThreadCandidate {
    pub(crate) reference: String,
    pub(crate) kind: String,
    pub(crate) score: f32,
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub(crate) async fn threads_for_card_candidates(
        self,
    ) -> Result<Vec<CandidateThread>, WorkThreadError> {
        let rows = crate::metrics::time_db("threads_for_card_candidates", async move {
            sqlx::query_as!(
                CandidateThread,
                r#"SELECT t.id AS "thread_id!", t.project_id AS "project_id!",
                          concat_ws(E'\n', NULLIF(btrim(t.title), ''), NULLIF(btrim(t.goal), ''),
                                    NULLIF(btrim(s.tldr), ''), NULLIF(btrim(s.where_we_are), ''),
                                    NULLIF(btrim(s.next_up), '')) AS "query_text!"
                     FROM work.threads t
                     LEFT JOIN work.thread_state s ON s.thread_id = t.id
                    WHERE t.status IS NULL
                       OR t.status NOT IN ('done', 'complete', 'parked', 'superseded')
                    ORDER BY t.id"#
            )
            .fetch_all(self.pool())
            .await
        })
        .await?;
        Ok(rows)
    }

    pub(crate) async fn thread_entry_ids_of(
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

    pub(crate) async fn replace_thread_card_candidates(
        self,
        thread_id: i64,
        candidates: &[NewThreadCandidate],
    ) -> Result<usize, WorkThreadError> {
        let references: Vec<String> = candidates.iter().map(|c| c.reference.clone()).collect();
        let kinds: Vec<String> = candidates.iter().map(|c| c.kind.clone()).collect();
        let scores: Vec<f32> = candidates.iter().map(|c| c.score).collect();
        let mut tx = self.pool().begin().await?;
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
