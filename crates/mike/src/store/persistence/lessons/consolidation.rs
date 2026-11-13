use super::LessonRow;
use crate::store::persistence::PersistError;
use crate::store::persistence::operator_turns::vector_to_text_literal;

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn claim_raw_lessons_for_consolidation(
        self,
        project_id: i64,
        batch_size: i64,
    ) -> Result<Vec<LessonRow>, PersistError> {
        crate::metrics::time_db("claim_raw_lessons_for_consolidation", async move {
            let mut tx = self.pool.begin().await?;
            let rows = sqlx::query_as!(
                LessonRow,
                r#"SELECT l.id AS "id!", l.project_id, p.project_path AS "project_path?", lk.code AS "kind!",
                          l.text AS "text!", l.agent_id, ls.code AS "stage!", l.source_turn_id,
                          l.created_at AS "created_at!", l.expires_at, l.topic
                     FROM memory.lessons l
                     LEFT JOIN projects.projects p ON p.id = l.project_id
                     JOIN memory.lesson_kinds lk ON lk.id = l.kind_id
                     JOIN memory.lesson_stages ls ON ls.id = l.stage_id
                    WHERE l.project_id = $1
                      AND ls.code = 'raw'
                      AND l.embedded_at IS NULL
                      AND l.source_id NOT IN (SELECT id FROM memory.lesson_sources WHERE code = 'carry')
                      AND NOT EXISTS (
                          SELECT 1 FROM memory.lesson_shape_findings f
                           WHERE f.lesson_id = l.id AND f.resolution IS NULL
                      )
                    ORDER BY l.created_at ASC
                    LIMIT $2
                    FOR UPDATE OF l SKIP LOCKED"#,
                project_id,
                batch_size,
            )
            .fetch_all(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(rows)
        })
        .await
    }

    #[rustfmt::skip]
    pub async fn promote_lesson_to_daily(self,
        id: i64,
        embedding: &[f32],
        model: &str,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("promote_lesson_to_daily", async move {
            let vec_literal = vector_to_text_literal(embedding);
            sqlx::query!(
                "UPDATE memory.lessons l
                    SET stage_id        = daily_s.id,
                        embedding       = $2::text::vector,
                        embedding_model = $3,
                        embedded_at     = NOW()
                   FROM memory.lesson_stages daily_s,
                        memory.lesson_stages raw_s
                  WHERE daily_s.code = 'daily'
                    AND raw_s.code = 'raw'
                    AND l.id = $1
                    AND l.stage_id = raw_s.id",
                id,
                vec_literal,
                model,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn promote_daily_to_long_term(
        self,
        project_id: i64,
        run_id: i64,
    ) -> Result<u64, PersistError> {
        crate::metrics::time_db("promote_daily_to_long_term", async move {
            let count = sqlx::query_scalar!(
                r#"WITH promoted AS (
                     UPDATE memory.lessons l
                        SET stage_id             = long_term_s.id,
                            consolidation_count  = consolidation_count + 1,
                            last_consolidated_at = NOW()
                       FROM memory.lesson_stages long_term_s,
                            memory.lesson_stages daily_s
                      WHERE long_term_s.code = 'long_term'
                        AND daily_s.code = 'daily'
                        AND l.project_id = $1
                        AND l.stage_id = daily_s.id
                        AND l.archived_at IS NULL
                     RETURNING l.id, l.uuid, l.text, l.project_id, l.agent_id, l.session_id, l.source_turn_id
                 ),
                 audit AS (
                     INSERT INTO memory.consolidation_actions (
                         run_id, lesson_id, lesson_uuid, project_id, agent_id, session_id, session_uuid,
                         source_turn_id, files_touched, lesson_text,
                         action_kind_id, outcome_id, before_stage_id, after_stage_id,
                         injection_count
                     )
                     SELECT $2::bigint, p.id, p.uuid, p.project_id, p.agent_id, p.session_id, sess.session_uuid,
                            p.source_turn_id, ot.files_touched, p.text,
                            ak.id, oc.id, daily_s.id, long_term_s.id,
                            COALESCE(inj.injection_count, 0)
                       FROM promoted p
                       LEFT JOIN sessions.sessions sess ON sess.id = p.session_id
                       LEFT JOIN memory.operator_turns ot ON ot.id = p.source_turn_id
                       JOIN memory.lesson_stages daily_s ON daily_s.code = 'daily'
                       JOIN memory.lesson_stages long_term_s ON long_term_s.code = 'long_term'
                       JOIN memory.consolidation_action_kinds ak ON ak.code = 'promoted_to_long_term'
                       JOIN memory.consolidation_outcomes oc ON oc.code = 'applied'
                       LEFT JOIN (
                           SELECT lesson_id, count(*)::integer AS injection_count
                             FROM memory.lesson_injections
                            GROUP BY lesson_id
                       ) inj ON inj.lesson_id = p.id
                     RETURNING id
                 )
                 SELECT count(*) AS "count!" FROM promoted"#,
                project_id,
                run_id,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(count as u64)
        })
        .await
    }
}
