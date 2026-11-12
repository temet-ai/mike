use uuid::Uuid;

use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn reconcile_runtime_mail_outbox(&self) -> Result<i64, PersistError> {
        Ok(
            sqlx::query_file_scalar!("src/store/persistence/runtime/reconcile_mail.sql")
                .fetch_one(self.pool())
                .await?,
        )
    }

    pub async fn runtime_mail_notification_state(
        &self,
        message: Uuid,
    ) -> Result<Option<String>, PersistError> {
        Ok(sqlx::query_scalar!(
            "SELECT n.state FROM mailbox.runtime_notifications n
             JOIN mailbox.messages m ON m.id = n.message_id WHERE m.message_uuid = $1",
            message
        )
        .fetch_optional(self.pool())
        .await?)
    }

    pub async fn reserve_runtime_mail_notification(
        &self,
        session: Uuid,
        message: Uuid,
    ) -> Result<bool, PersistError> {
        Ok(sqlx::query_scalar!(
            "INSERT INTO mailbox.runtime_notifications (message_id, state)
             SELECT m.id, 'attempted' FROM mailbox.messages m
             JOIN sessions.sessions s ON s.id = m.to_session_id
             WHERE s.session_uuid = $1 AND m.message_uuid = $2
               AND m.consumed_at IS NULL AND m.dead_lettered_at IS NULL
             ON CONFLICT (message_id) DO NOTHING RETURNING message_id",
            session,
            message
        )
        .fetch_optional(self.pool())
        .await?
        .is_some())
    }

    pub async fn finish_runtime_mail_notification(
        &self,
        message: Uuid,
        state: &str,
        detail: Option<&str>,
    ) -> Result<(), PersistError> {
        if !matches!(state, "queued" | "uncertain" | "failed")
            || detail.is_some_and(|d| d.len() > 4096)
        {
            return Err(PersistError::DataInvariant(
                "invalid runtime notification result".into(),
            ));
        }
        sqlx::query!(
            "UPDATE mailbox.runtime_notifications n SET state = $2, completed_at = now(), detail = $3
             FROM mailbox.messages m WHERE n.message_id = m.id AND m.message_uuid = $1
               AND n.state = 'attempted'",
            message, state, detail
        ).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn pending_runtime_mail(
        &self,
        session: Uuid,
        limit: i64,
    ) -> Result<Vec<Uuid>, PersistError> {
        Ok(sqlx::query_scalar!(
            "SELECT m.message_uuid FROM mailbox.messages m
             JOIN sessions.sessions s ON s.id = m.to_session_id
             WHERE s.session_uuid = $1 AND m.consumed_at IS NULL AND m.dead_lettered_at IS NULL
             ORDER BY m.id LIMIT $2",
            session,
            limit.clamp(1, 20)
        )
        .fetch_all(self.pool())
        .await?)
    }
}
