use super::super::PersistError;

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    #[rustfmt::skip]
    pub async fn record_lesson_injections(self,
        request_id: i64,
        session_uuid: &str,
        lane: &str,
        persona: Option<&str>,
        lesson_ids: &[i64],
    ) -> Result<(), PersistError> {
        if lesson_ids.is_empty() {
            return Ok(());
        }
        crate::metrics::time_db("record_lesson_injections", async move {
            sqlx::query!(
                "INSERT INTO memory.lesson_injections
                    (lesson_id, request_id, session_id, lane_id, persona)
                 SELECT u.lesson_id, $1::bigint, s.id, lane.id, $4::text
                   FROM UNNEST($2::bigint[]) AS u(lesson_id)
                   JOIN memory.lesson_injection_lanes lane ON lane.code = $3
              LEFT JOIN sessions.sessions s ON s.session_uuid = $5::text::uuid",
                request_id,
                lesson_ids,
                lane,
                persona,
                session_uuid,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }
}
