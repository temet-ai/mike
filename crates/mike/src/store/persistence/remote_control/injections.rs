use super::PersistError;
use chrono::{DateTime, Utc};
use uuid::Uuid;

pub enum PushSource {
    Mailbox { message_uuid: Uuid },
    Steerer { intervention_uuid: Uuid },
    Lesson,
    Diagnostic,
    Recall,
}

impl PushSource {
    pub(crate) fn kind(&self) -> &'static str {
        match self {
            PushSource::Mailbox { .. } => "mailbox",
            PushSource::Steerer { .. } => "steerer",
            PushSource::Lesson => "lesson",
            PushSource::Diagnostic => "diagnostic",
            PushSource::Recall => "recall",
        }
    }
}

impl crate::store::persistence::proxy_repo::ProxyDb<'_> {
    pub async fn reserve_remote_control_injection(
        self,
        cse_id: &str,
        session_uuid: Uuid,
        source: &PushSource,
        event_id: Uuid,
        frame: &[u8],
    ) -> Result<Option<DateTime<Utc>>, PersistError> {
        crate::metrics::time_db("remote_control_reserve_injection", async {
            let reserved = match source {
                PushSource::Mailbox { message_uuid } => sqlx::query_file!(
                    "src/store/persistence/remote_control/reserve_injection.sql",
                    cse_id,
                    session_uuid,
                    message_uuid,
                    event_id,
                    frame,
                )
                .fetch_optional(&self.pool)
                .await?
                .map(|row| row.created_at),
                PushSource::Steerer { intervention_uuid } => sqlx::query_file!(
                    "src/store/persistence/remote_control/reserve_steerer_injection.sql",
                    cse_id,
                    session_uuid,
                    intervention_uuid,
                    event_id,
                    frame,
                )
                .fetch_optional(&self.pool)
                .await?
                .map(|row| row.created_at),
                PushSource::Lesson | PushSource::Diagnostic | PushSource::Recall => {
                    sqlx::query_file!(
                        "src/store/persistence/remote_control/reserve_unclaimed_injection.sql",
                        cse_id,
                        session_uuid,
                        source.kind(),
                        event_id,
                        frame,
                    )
                    .fetch_optional(&self.pool)
                    .await?
                    .map(|row| row.created_at)
                }
            };
            Ok(reserved)
        })
        .await
    }

    pub async fn finish_remote_control_injection(
        self,
        event_id: Uuid,
        emitted: bool,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("remote_control_finish_injection", async {
            let mut tx = self.pool.begin().await?;
            sqlx::query_file!(
                "src/store/persistence/remote_control/finish_injection.sql",
                event_id,
                emitted
            )
            .execute(&mut *tx)
            .await?;
            sqlx::query_file!(
                "src/store/persistence/remote_control/finish_steerer_injection.sql",
                event_id,
                emitted
            )
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(())
        })
        .await
    }

    pub async fn ack_remote_control_injections(
        self,
        cse_id: &str,
        event_ids: &[Uuid],
        observed_at: DateTime<Utc>,
        call_id: i64,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("remote_control_ack_injections", async {
            let mut tx = self.pool.begin().await?;
            sqlx::query_file!(
                "src/store/persistence/remote_control/ack_injections.sql",
                cse_id,
                event_ids,
                observed_at,
                call_id,
            )
            .execute(&mut *tx)
            .await?;
            sqlx::query_file!(
                "src/store/persistence/remote_control/ack_steering_interventions.sql",
                cse_id,
                event_ids,
                observed_at,
            )
            .execute(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(())
        })
        .await
    }

    pub async fn release_stale_steerer_push(
        self,
        intervention_uuid: Uuid,
        expected_pushed_at: DateTime<Utc>,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("remote_control_release_stale_steerer_push", async {
            let released = sqlx::query_file_scalar!(
                "src/store/persistence/remote_control/release_stale_steerer_push.sql",
                intervention_uuid,
                expected_pushed_at,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(released.is_some())
        })
        .await
    }

    pub async fn release_stale_mailbox_push(
        self,
        event_id: Uuid,
        max_attempts: i32,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("remote_control_release_stale_mailbox_push", async {
            let released = sqlx::query_file_scalar!(
                "src/store/persistence/remote_control/release_stale_mailbox_push.sql",
                event_id,
                max_attempts,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(released.is_some())
        })
        .await
    }

    pub async fn stamp_push_carriers(
        self,
        session_uuid: Uuid,
        request_id: i64,
        request_started_at: DateTime<Utc>,
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db("remote_control_stamp_push_carriers", async {
            let carried = sqlx::query_file_scalar!(
                "src/store/persistence/remote_control/stamp_push_carriers.sql",
                session_uuid,
                request_id,
                request_started_at,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(carried)
        })
        .await
    }
}
