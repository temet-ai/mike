use crate::store::persistence::PersistError;

use super::types::MailboxMessageRow;

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn fetch_mail_for_drain(
        self,
        to_session_uuid: uuid::Uuid,
        limit: i64,
    ) -> Result<Vec<MailboxMessageRow>, PersistError> {
        crate::metrics::time_db("mailbox_fetch_mail_for_drain", async move {
            let rows = sqlx::query!(
                r#"SELECT
                       m.message_uuid      AS "message_uuid!",
                       recip.session_uuid  AS "to_session_uuid!",
                       sender.session_uuid AS "from_session_uuid?",
                       m.body              AS "body!",
                       m.queued_at         AS "queued_at!",
                       m.delivery_attempts AS "delivery_attempts!"
                   FROM mailbox.messages m
                   JOIN sessions.sessions recip  ON recip.id  = m.to_session_id
                   LEFT JOIN sessions.sessions sender ON sender.id = m.from_session_id
                  WHERE recip.session_uuid = $1
                    AND m.sent_request_id IS NULL
                    AND m.channel_delivered_at IS NULL
                    AND m.claimed_at IS NULL
                    AND m.consumed_at IS NULL
                    AND m.dead_lettered_at IS NULL
                    AND NOT EXISTS (
                        SELECT 1 FROM launch.launches l
                         WHERE l.session_uuid = $1
                           AND l.brief_note_uuid = m.message_uuid)
                  ORDER BY m.id ASC
                  LIMIT $2"#,
                to_session_uuid,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| MailboxMessageRow {
                    message_uuid: r.message_uuid,
                    to_session_uuid: r.to_session_uuid,
                    from_session_uuid: r.from_session_uuid,
                    body: r.body,
                    queued_at: r.queued_at,
                    claimed_at: None,
                    sent_request_id: None,
                    sent_at: None,
                    consumed_at: None,
                    dead_lettered_at: None,
                    channel_delivered_at: None,
                    delivery_attempts: r.delivery_attempts,
                    thread_uuid: None,
                    in_reply_to_uuid: None,
                })
                .collect())
        })
        .await
    }
}
