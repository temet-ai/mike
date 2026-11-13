use super::super::WorkThreadError;
use super::DirtyThread;

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub(crate) async fn thread_card_sources_generation(self) -> Result<i64, WorkThreadError> {
        let generation = crate::metrics::time_db("thread_card_sources_generation", async move {
            sqlx::query_scalar!(
                r#"SELECT generation AS "generation!"
                     FROM memory.change_generations
                    WHERE name = 'thread_card_sources'"#
            )
            .fetch_one(self.pool())
            .await
        })
        .await?;
        Ok(generation)
    }

    pub(crate) async fn thread_cards_behind_their_threads(
        self,
    ) -> Result<Vec<DirtyThread>, WorkThreadError> {
        let rows = crate::metrics::time_db("thread_cards_behind_their_threads", async move {
            sqlx::query_as!(
                DirtyThread,
                r#"SELECT t.id AS "thread_id!",
                          GREATEST(t.updated_at, s.updated_at, l.max_at, ts.max_at, cc.max_at) AS "observed_at!"
                     FROM work.threads t
                     LEFT JOIN work.thread_state s ON s.thread_id = t.id
                     LEFT JOIN work.thread_cards c ON c.thread_id = t.id
                     LEFT JOIN LATERAL (
                          SELECT max(lg.created_at) AS max_at
                            FROM work.thread_log lg
                           WHERE lg.thread_id = t.id
                     ) l ON true
                     LEFT JOIN LATERAL (
                          SELECT max(x.first_seen_at) AS max_at
                            FROM work.thread_sessions x
                           WHERE x.thread_id = t.id
                     ) ts ON true
                     LEFT JOIN LATERAL (
                          SELECT max(k.built_at) AS max_at
                            FROM work.thread_card_candidates k
                           WHERE k.thread_id = t.id
                     ) cc ON true
                    WHERE (t.status IS NULL OR t.status NOT IN ('done', 'complete'))
                      AND (c.thread_id IS NULL
                           OR GREATEST(t.updated_at, s.updated_at, l.max_at, ts.max_at, cc.max_at)
                              > c.source_max_at)
                    ORDER BY t.id"#
            )
            .fetch_all(self.pool())
            .await
        })
        .await?;
        Ok(rows)
    }

    pub(crate) async fn thread_cards_behind_their_busy_sources(
        self,
    ) -> Result<Vec<DirtyThread>, WorkThreadError> {
        let rows = crate::metrics::time_db("thread_cards_behind_their_busy_sources", async move {
            sqlx::query_as!(
                DirtyThread,
                r#"WITH open_threads AS (
                       SELECT t.id, t.project_id, 'thread:' || t.slug AS tag
                         FROM work.threads t
                         JOIN work.thread_cards c ON c.thread_id = t.id
                        WHERE t.status IS NULL OR t.status NOT IN ('done', 'complete')
                   ),
                   session_times AS (
                       SELECT o.id AS thread_id, GREATEST(max(p.max_at), max(v.max_at)) AS max_at
                         FROM open_threads o
                         JOIN work.thread_sessions ts ON ts.thread_id = o.id
                         JOIN sessions.sessions se ON se.session_uuid = ts.session_uuid
                         LEFT JOIN LATERAL (
                              SELECT max(GREATEST(d.recorded_at, d.close_recorded_at)) AS max_at
                                FROM memory.dossier_entries d
                               WHERE d.session_id = se.id AND d.kind = 'promise'
                         ) p ON true
                         LEFT JOIN LATERAL (
                              SELECT v.created_at AS max_at
                                FROM memory.turn_verdicts v
                               WHERE v.session_id = se.id AND v.mode = 'live'
                               ORDER BY v.created_at DESC
                               LIMIT 1
                         ) v ON true
                        GROUP BY o.id
                   ),
                   note_times AS (
                       SELECT o.id AS thread_id, max(n.updated_at) AS max_at
                         FROM open_threads o
                         JOIN workspace.session_notes n
                           ON n.tags @> ARRAY[o.tag] AND n.kind IN ('decision', 'finding')
                         JOIN sessions.sessions se ON se.id = n.session_id AND se.project_id = o.project_id
                        GROUP BY o.id
                   ),
                   busy AS (
                       SELECT st.thread_id, st.max_at FROM session_times st
                       UNION ALL
                       SELECT nt.thread_id, nt.max_at FROM note_times nt
                   )
                   SELECT b.thread_id AS "thread_id!", max(b.max_at) AS "observed_at!"
                     FROM busy b
                     JOIN work.thread_cards c ON c.thread_id = b.thread_id
                    GROUP BY b.thread_id, c.source_max_at
                   HAVING max(b.max_at) > c.source_max_at
                    ORDER BY b.thread_id"#
            )
            .fetch_all(self.pool())
            .await
        })
        .await?;
        Ok(rows)
    }
}
