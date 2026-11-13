use crate::store::persistence::PersistError;

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn gc_expired_lessons(self, run_id: i64) -> Result<(u64, u64), PersistError> {
        crate::metrics::time_db("gc_expired_lessons", async move {
            let archived_count: i64 = sqlx::query_scalar(
                "WITH archived AS (
                     UPDATE memory.lessons l
                        SET stage_id = archived_s.id,
                            archived_at = NOW(),
                            archived_reason_id = expired_r.id
                       FROM memory.lesson_stages archived_s,
                            memory.lesson_archived_reasons expired_r,
                            memory.lesson_stages current_s
                      WHERE archived_s.code = 'archived'
                        AND expired_r.code = 'expired'
                        AND current_s.id = l.stage_id
                        AND l.expires_at IS NOT NULL AND l.expires_at < NOW()
                        AND l.stage_id != archived_s.id
                     RETURNING l.id, l.uuid, l.text, l.project_id, l.agent_id, l.session_id,
                               l.source_turn_id, current_s.code AS before_stage_code
                 ),
                 audit AS (
                     INSERT INTO memory.consolidation_actions (
                         run_id, lesson_id, lesson_uuid, project_id, agent_id, session_id, session_uuid,
                         source_turn_id, files_touched, lesson_text,
                         action_kind_id, outcome_id, before_stage_id, after_stage_id, decided_by_id,
                         injection_count
                     )
                     SELECT $1, a.id, a.uuid, a.project_id, a.agent_id, a.session_id, sess.session_uuid,
                            a.source_turn_id, ot.files_touched, a.text,
                            ak.id, oc.id, bs.id, after_s.id, db.id,
                            COALESCE(inj.injection_count, 0)
                       FROM archived a
                       LEFT JOIN sessions.sessions sess ON sess.id = a.session_id
                       LEFT JOIN memory.operator_turns ot ON ot.id = a.source_turn_id
                       JOIN memory.lesson_stages bs ON bs.code = a.before_stage_code
                       JOIN memory.lesson_stages after_s ON after_s.code = 'archived'
                       JOIN memory.consolidation_action_kinds ak ON ak.code = 'archived_expired'
                       JOIN memory.consolidation_outcomes oc ON oc.code = 'applied'
                       JOIN memory.consolidation_decided_by db ON db.code = 'expiry'
                       LEFT JOIN (
                           SELECT lesson_id, count(*)::integer AS injection_count
                             FROM memory.lesson_injections
                            GROUP BY lesson_id
                       ) inj ON inj.lesson_id = a.id
                     RETURNING id
                 )
                 SELECT count(*) FROM archived",
            )
            .bind(run_id)
            .fetch_one(&self.pool)
            .await?;

            Ok((archived_count as u64, 0))
        })
        .await
    }

    pub async fn trim_lessons_to_max(
        self,
        project_id: i64,
        max_count: i64,
        run_id: i64,
    ) -> Result<u64, PersistError> {
        crate::metrics::time_db("trim_lessons_to_max", async move {
            let count: i64 = sqlx::query_scalar(
                "WITH archived_stage AS (
                     SELECT id FROM memory.lesson_stages WHERE code = 'archived'
                 ),
                 trimmed_reason AS (
                     SELECT id FROM memory.lesson_archived_reasons WHERE code = 'trimmed'
                 ),
                 active AS (
                     SELECT l.id, l.stage_id, l.consolidation_count, l.created_at,
                            ROW_NUMBER() OVER (
                                ORDER BY l.consolidation_count ASC, l.created_at ASC
                            ) AS rn
                       FROM memory.lessons l, archived_stage
                      WHERE l.project_id = $1
                        AND l.stage_id != archived_stage.id
                 ),
                 excess AS (
                     SELECT GREATEST(0, COUNT(*)::bigint - $2) AS n
                       FROM memory.lessons l, archived_stage
                      WHERE l.project_id = $1
                        AND l.stage_id != archived_stage.id
                 ),
                 to_trim AS (
                     SELECT a.id, a.stage_id, a.consolidation_count, a.created_at, a.rn, e.n
                       FROM active a, excess e
                      WHERE a.rn <= e.n
                 ),
                 trimmed AS (
                     UPDATE memory.lessons l
                        SET stage_id = archived_stage.id,
                            archived_at = NOW(),
                            archived_reason_id = trimmed_reason.id
                       FROM to_trim, archived_stage, trimmed_reason, memory.lesson_stages current_s
                      WHERE l.id = to_trim.id
                        AND current_s.id = to_trim.stage_id
                     RETURNING l.id, l.uuid, l.text, l.project_id, l.agent_id, l.session_id,
                               l.source_turn_id, current_s.code AS before_stage_code,
                               to_trim.consolidation_count, to_trim.created_at, to_trim.rn, to_trim.n
                 ),
                 audit AS (
                     INSERT INTO memory.consolidation_actions (
                         run_id, lesson_id, lesson_uuid, project_id, agent_id, session_id, session_uuid,
                         source_turn_id, files_touched, lesson_text,
                         action_kind_id, outcome_id, before_stage_id, after_stage_id, decided_by_id,
                         rank_detail, injection_count
                     )
                     SELECT $3, t.id, t.uuid, t.project_id, t.agent_id, t.session_id, sess.session_uuid,
                            t.source_turn_id, ot.files_touched, t.text,
                            ak.id, oc.id, bs.id, after_s.id, db.id,
                            jsonb_build_object(
                                'rank', t.rn,
                                'excess_n', t.n,
                                'consolidation_count', t.consolidation_count,
                                'created_at', t.created_at
                            ),
                            COALESCE(inj.injection_count, 0)
                       FROM trimmed t
                       LEFT JOIN sessions.sessions sess ON sess.id = t.session_id
                       LEFT JOIN memory.operator_turns ot ON ot.id = t.source_turn_id
                       JOIN memory.lesson_stages bs ON bs.code = t.before_stage_code
                       JOIN memory.lesson_stages after_s ON after_s.code = 'archived'
                       JOIN memory.consolidation_action_kinds ak ON ak.code = 'archived_trimmed'
                       JOIN memory.consolidation_outcomes oc ON oc.code = 'applied'
                       JOIN memory.consolidation_decided_by db ON db.code = 'rank_order'
                       LEFT JOIN (
                           SELECT lesson_id, count(*)::integer AS injection_count
                             FROM memory.lesson_injections
                            GROUP BY lesson_id
                       ) inj ON inj.lesson_id = t.id
                     RETURNING id
                 )
                 SELECT count(*) FROM trimmed",
            )
            .bind(project_id)
            .bind(max_count)
            .bind(run_id)
            .fetch_one(&self.pool)
            .await?;
            Ok(count as u64)
        })
        .await
    }
}
