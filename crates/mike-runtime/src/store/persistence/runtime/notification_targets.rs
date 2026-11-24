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
        sqlx::query_file!(
            "src/store/persistence/runtime/register_notification_target.sql",
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
        Ok(sqlx::query_file_as!(
            RuntimeNotificationTarget,
            "src/store/persistence/runtime/read_notification_target.sql",
            session
        )
        .fetch_optional(self.pool())
        .await?)
    }
}

#[cfg(test)]
#[path = "notification_target_tests.rs"]
mod tests;
