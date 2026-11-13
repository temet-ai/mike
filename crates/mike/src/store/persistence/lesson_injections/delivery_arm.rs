use super::super::PersistError;

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn record_lesson_delivery(
        self,
        session_uuid: &str,
        arm: &str,
        lessons: i32,
        bytes: i64,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("record_lesson_delivery", async move {
            sqlx::query!(
                "INSERT INTO memory.lesson_delivery_arms
                    (session_uuid, arm, last_delivery_at, deliveries, lessons_delivered, bytes_delivered)
                 VALUES ($1::text::uuid, $2,
                         CASE WHEN $3 > 0 THEN now() END,
                         CASE WHEN $3 > 0 THEN 1 ELSE 0 END, $3, $4)
                 ON CONFLICT (session_uuid) DO UPDATE SET
                    last_delivery_at = CASE WHEN $3 > 0 THEN now()
                                            ELSE memory.lesson_delivery_arms.last_delivery_at END,
                    deliveries = memory.lesson_delivery_arms.deliveries
                               + CASE WHEN $3 > 0 THEN 1 ELSE 0 END,
                    lessons_delivered = memory.lesson_delivery_arms.lessons_delivered + $3,
                    bytes_delivered = memory.lesson_delivery_arms.bytes_delivered + $4",
                session_uuid,
                arm,
                lessons,
                bytes,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }
}
