use super::super::{PersistError, PersistHandle};

impl PersistHandle {
    #[rustfmt::skip]
    pub async fn record_mod_lesson_injections(
        &self,
        request_id: i64,
        session_uuid: &str,
        lane: &str,
        lesson_ids: &[i64],
    ) -> Result<u64, PersistError> {
        if lesson_ids.is_empty() {
            return Ok(0);
        }
        crate::metrics::time_db("record_mod_lesson_injections", async move {
            let result = sqlx::query!(
                "INSERT INTO memory.lesson_injections
                    (lesson_id, request_id, session_id, lane_id, origin)
                 SELECT l.id, $1::bigint, s.id, lane.id, 'mod'
                   FROM UNNEST($2::bigint[]) AS u(lesson_id)
                   JOIN memory.lessons l ON l.id = u.lesson_id
                   JOIN sessions.sessions s ON s.session_uuid = $3::text::uuid
                   JOIN memory.lesson_injection_lanes lane ON lane.code = $4
                  WHERE NOT EXISTS (
                        SELECT 1 FROM memory.lesson_injections seen
                         WHERE seen.session_id = s.id
                           AND seen.lesson_id = l.id
                           AND seen.origin = 'mod')
                 ON CONFLICT (lesson_id, request_id) DO NOTHING",
                request_id,
                lesson_ids,
                session_uuid,
                lane,
            )
            .execute(&self.pool)
            .await?;
            Ok(result.rows_affected())
        })
        .await
    }
}
