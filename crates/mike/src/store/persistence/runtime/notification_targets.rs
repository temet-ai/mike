use uuid::Uuid;

use super::ObservationLimits;
use crate::store::persistence::PersistError;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct RuntimeNotificationTarget {
    pub thread_id: String,
    pub runtime_home: String,
}

impl crate::store::persistence::runtime_repo::RuntimeDb<'_> {
    pub async fn register_runtime_notification_target(
        self,
        session: Uuid,
        thread: &str,
        home: &str,
    ) -> Result<(), PersistError> {
        if thread.is_empty() || home.is_empty() || thread.len() + home.len() > 8192 {
            return Err(PersistError::DataInvariant(
                "invalid runtime notification target".into(),
            ));
        }
        let limits = ObservationLimits::default();
        let mut tx = self.pool().begin().await?;
        let used = sqlx::query!(
            "SELECT retained_rows, retained_bytes FROM runtime.storage_accounting WHERE kind = 'facts' FOR UPDATE"
        ).fetch_one(&mut *tx).await?;
        if used.retained_rows + 1 > limits.max_rows
            || used.retained_bytes + (thread.len() + home.len()) as i64 + 128 > limits.max_bytes
        {
            return Err(PersistError::DataInvariant(
                "runtime observation quota exhausted".into(),
            ));
        }
        sqlx::query!(
            "INSERT INTO runtime.notification_targets (session_id, thread_id, runtime_home)
             SELECT s.id, $2, $3 FROM sessions.sessions s
             JOIN runtime.session_facts f ON f.session_id = s.id
             WHERE s.session_uuid = $1 AND f.control = 'hook_boundary'
             ON CONFLICT (session_id) DO UPDATE SET thread_id = EXCLUDED.thread_id,
                 runtime_home = EXCLUDED.runtime_home, verified_at = now()",
            session,
            thread,
            home
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn runtime_notification_target(
        self,
        session: Uuid,
    ) -> Result<Option<RuntimeNotificationTarget>, PersistError> {
        Ok(sqlx::query_as!(
            RuntimeNotificationTarget,
            "SELECT t.thread_id, t.runtime_home FROM runtime.notification_targets t
             JOIN sessions.sessions s ON s.id = t.session_id
             JOIN runtime.session_facts f ON f.session_id = t.session_id
             WHERE s.session_uuid = $1 AND f.control = 'hook_boundary'
               AND f.lifecycle IS DISTINCT FROM 'ended' AND f.lifecycle IS DISTINCT FROM 'failed'
               AND t.verified_at > now() - interval '7 days'",
            session
        )
        .fetch_optional(self.pool())
        .await?)
    }
}
