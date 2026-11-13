use chrono::{DateTime, Utc};

use crate::store::persistence::PersistError;

use super::thread_types::{MessageCore, ResolveThreadOutcome, ThreadCore};

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub(crate) async fn resolve_session_core(
        self,
        session_uuid: uuid::Uuid,
    ) -> Result<Option<(i64, i64)>, PersistError> {
        crate::metrics::time_db("mailbox_resolve_session_core", async move {
            let row = sqlx::query!(
                r#"SELECT id, project_id AS "project_id!"
                     FROM sessions.sessions WHERE session_uuid = $1"#,
                session_uuid,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|row| (row.id, row.project_id)))
        })
        .await
    }

    pub(crate) async fn fetch_thread_core_by_uuid(
        self,
        thread_uuid: uuid::Uuid,
    ) -> Result<Option<ThreadCore>, PersistError> {
        crate::metrics::time_db("mailbox_fetch_thread_core_by_uuid", async move {
            let row = sqlx::query_as!(
                ThreadCore,
                "SELECT id, thread_uuid, subject FROM mailbox.threads WHERE thread_uuid = $1",
                thread_uuid,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    pub(super) async fn fetch_thread_core_by_id(
        self,
        id: i64,
    ) -> Result<Option<ThreadCore>, PersistError> {
        crate::metrics::time_db("mailbox_fetch_thread_core_by_id", async move {
            let row = sqlx::query_as!(
                ThreadCore,
                "SELECT id, thread_uuid, subject FROM mailbox.threads WHERE id = $1",
                id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    pub(crate) async fn fetch_message_core_by_uuid(
        self,
        message_uuid: uuid::Uuid,
    ) -> Result<Option<MessageCore>, PersistError> {
        crate::metrics::time_db("mailbox_fetch_message_core_by_uuid", async move {
            let row = sqlx::query_as!(
                MessageCore,
                "SELECT id, thread_id, body FROM mailbox.messages WHERE message_uuid = $1",
                message_uuid,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    pub(crate) async fn create_thread(
        self,
        thread_uuid: uuid::Uuid,
        subject: &str,
        project_id: Option<i64>,
        created_by_session_id: Option<i64>,
    ) -> Result<ThreadCore, PersistError> {
        crate::metrics::time_db("mailbox_create_thread", async move {
            let id = sqlx::query_scalar!(
                "INSERT INTO mailbox.threads (thread_uuid, subject, project_id, created_by_session_id)
                 VALUES ($1, $2, $3, $4)
                 RETURNING id",
                thread_uuid,
                subject,
                project_id,
                created_by_session_id,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(ThreadCore {
                id,
                thread_uuid,
                subject: subject.to_string(),
            })
        })
        .await
    }

    pub(super) async fn adopt_message_into_thread(
        self,
        message_id: i64,
        thread_id: i64,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("mailbox_adopt_message_into_thread", async move {
            sqlx::query!(
                "UPDATE mailbox.messages SET thread_id = $1 WHERE id = $2 AND thread_id IS NULL",
                thread_id,
                message_id,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn touch_thread(self, thread_id: i64) -> Result<(), PersistError> {
        crate::metrics::time_db("mailbox_touch_thread", async move {
            sqlx::query!(
                "UPDATE mailbox.threads SET last_message_at = now() WHERE id = $1",
                thread_id,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub(crate) async fn mark_thread_resolved(
        self,
        thread_uuid: uuid::Uuid,
    ) -> Result<ResolveThreadOutcome, PersistError> {
        crate::metrics::time_db("mailbox_mark_thread_resolved", async move {
            let updated: Option<DateTime<Utc>> = sqlx::query_scalar!(
                r#"UPDATE mailbox.threads SET resolved_at = now()
                    WHERE thread_uuid = $1 AND resolved_at IS NULL
                RETURNING resolved_at AS "resolved_at!""#,
                thread_uuid,
            )
            .fetch_optional(&self.pool)
            .await?;
            if let Some(resolved_at) = updated {
                return Ok(ResolveThreadOutcome::Resolved(resolved_at));
            }
            let existing = sqlx::query_scalar!(
                "SELECT resolved_at FROM mailbox.threads WHERE thread_uuid = $1",
                thread_uuid,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(match existing {
                Some(Some(resolved_at)) => ResolveThreadOutcome::Resolved(resolved_at),
                None | Some(None) => ResolveThreadOutcome::NotFound,
            })
        })
        .await
    }
}
