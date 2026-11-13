use sqlx::{Executor, Postgres};

use crate::store::persistence::PersistError;

pub async fn tell_listed_senders<'e>(
    executor: impl Executor<'e, Database = Postgres>,
    messages: &[uuid::Uuid],
) -> Result<u64, PersistError> {
    if messages.is_empty() {
        return Ok(0);
    }
    let notified: i64 = sqlx::query_file_scalar!(
        "src/store/persistence/mailbox/undelivered_notice_listed.sql",
        messages
    )
    .fetch_one(executor)
    .await?;
    Ok(notified.unsigned_abs())
}

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn tell_sender_not_delivered(
        self,
        message_uuid: uuid::Uuid,
    ) -> Result<bool, PersistError> {
        Ok(self.tell_senders_not_delivered(&[message_uuid]).await? > 0)
    }

    pub async fn tell_senders_not_delivered(
        self,
        messages: &[uuid::Uuid],
    ) -> Result<u64, PersistError> {
        crate::metrics::time_db(
            "mailbox_tell_sender_not_delivered",
            tell_listed_senders(&self.pool, messages),
        )
        .await
    }

    pub async fn tell_inactive_senders(
        self,
        cutoff: chrono::DateTime<chrono::Utc>,
    ) -> Result<u64, PersistError> {
        crate::metrics::time_db("mailbox_tell_inactive_senders", async move {
            let mut tx = self.background_pool().begin().await?;
            let due = sqlx::query_file_scalar!(
                "src/store/persistence/mailbox/undelivered_inactive_due.sql",
                cutoff
            )
            .fetch_all(&mut *tx)
            .await?;
            let notified = tell_listed_senders(&mut *tx, &due).await?;
            tx.commit().await?;
            Ok(notified)
        })
        .await
    }

    pub async fn revive_undelivered_mail(
        self,
        to_session_uuid: uuid::Uuid,
    ) -> Result<Vec<uuid::Uuid>, PersistError> {
        crate::metrics::time_db("mailbox_revive_undelivered", async move {
            let revived = sqlx::query_scalar!(
                r#"UPDATE mailbox.messages m
                      SET dead_lettered_at = NULL,
                          claimed_at = NULL,
                          queued_at = now()
                     FROM sessions.sessions recip
                    WHERE recip.id = m.to_session_id
                      AND recip.session_uuid = $1
                      AND m.dead_lettered_at IS NOT NULL
                      AND m.consumed_at IS NULL
                      AND m.sent_at IS NULL
                      AND m.sent_request_id IS NULL
                      AND m.channel_delivered_at IS NULL
                RETURNING m.message_uuid AS "message_uuid!""#,
                to_session_uuid,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(revived)
        })
        .await
    }
}
