use super::super::PersistError;

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn record_lesson_delivery(
        self,
        session_uuid: &str,
        lessons: i32,
        bytes: i64,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("record_lesson_delivery", async move {
            sqlx::query!(
                "INSERT INTO memory.lesson_delivery_arms
                    (session_uuid, arm, last_call_at, last_delivery_at, deliveries,
                     lessons_delivered, bytes_delivered)
                 VALUES ($1::text::uuid, 'mod', now(),
                         CASE WHEN $2 > 0 THEN now() END,
                         CASE WHEN $2 > 0 THEN 1 ELSE 0 END, $2, $3)
                 ON CONFLICT (session_uuid) DO UPDATE SET
                    last_call_at = now(),
                    last_delivery_at = CASE WHEN $2 > 0 THEN now()
                                            ELSE memory.lesson_delivery_arms.last_delivery_at END,
                    deliveries = memory.lesson_delivery_arms.deliveries
                               + CASE WHEN $2 > 0 THEN 1 ELSE 0 END,
                    lessons_delivered = memory.lesson_delivery_arms.lessons_delivered + $2,
                    bytes_delivered = memory.lesson_delivery_arms.bytes_delivered + $3",
                session_uuid,
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

impl crate::store::persistence::lessons_repo::LessonsDb<'_> {
    pub async fn record_lesson_client_declared(
        self,
        session_uuid: &str,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("record_lesson_client_declared", async move {
            sqlx::query!(
                "INSERT INTO memory.lesson_delivery_arms (session_uuid, arm, declared_at)
                 VALUES ($1::text::uuid, 'mod', now())
                 ON CONFLICT (session_uuid) DO UPDATE SET declared_at = now()",
                session_uuid,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn recent_declared_sessions(
        self,
        since: chrono::DateTime<chrono::Utc>,
    ) -> Result<Vec<String>, PersistError> {
        crate::metrics::time_db("recent_declared_sessions", async move {
            Ok(sqlx::query_scalar!(
                r#"SELECT session_uuid::text AS "session!"
                     FROM memory.lesson_delivery_arms
                    WHERE declared_at >= $1"#,
                since,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}
