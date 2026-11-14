use crate::store::persistence::PersistError;

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn reply_answers_session(
        self,
        in_reply_to_id: i64,
        session_id: i64,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("mailbox_reply_answers_session", async move {
            let found = sqlx::query_scalar!(
                r#"SELECT EXISTS (
                       SELECT 1 FROM mailbox.messages p
                        WHERE p.id = $1 AND p.from_session_id = $2
                   ) AS "found!""#,
                in_reply_to_id,
                session_id,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(found)
        })
        .await
    }

    pub async fn release_held_mail(self, to_session_uuid: uuid::Uuid) -> Result<u64, PersistError> {
        crate::metrics::time_db("mailbox_release_held_mail", async move {
            let done = sqlx::query!(
                "UPDATE mailbox.messages m
                    SET held_until_turn = false
                   FROM sessions.sessions s
                  WHERE s.id = m.to_session_id
                    AND s.session_uuid = $1
                    AND m.held_until_turn
                    AND m.sent_request_id IS NULL
                    AND m.channel_delivered_at IS NULL
                    AND m.consumed_at IS NULL
                    AND m.dead_lettered_at IS NULL",
                to_session_uuid,
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }
}
