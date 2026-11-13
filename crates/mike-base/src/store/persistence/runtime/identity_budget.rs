use sqlx::PgConnection;
use uuid::Uuid;

use crate::store::persistence::{PersistError, PersistHandle, id_resolvers};

pub async fn lock(connection: &mut PgConnection) -> Result<(), sqlx::Error> {
    sqlx::query!("SELECT kind FROM runtime.storage_accounting WHERE kind = 'facts' FOR UPDATE")
        .fetch_one(connection)
        .await?;
    Ok(())
}

pub async fn admit(
    connection: &mut PgConnection,
    limits: &ObservationLimits,
) -> Result<(), PersistError> {
    let used = sqlx::query!(
        "SELECT retained_rows, retained_bytes FROM runtime.storage_accounting WHERE kind = 'facts'"
    )
    .fetch_one(connection)
    .await?;
    if used.retained_rows > limits.max_rows - (limits.max_rows / 10).min(1000)
        || used.retained_bytes > limits.max_bytes - (limits.max_bytes / 10).min(1024 * 1024)
    {
        return Err(PersistError::DataInvariant(
            "runtime identity storage budget exhausted".into(),
        ));
    }
    Ok(())
}

impl PersistHandle {
    pub async fn end_runtime_workspace_registration(
        &self,
        session: Uuid,
    ) -> Result<(), PersistError> {
        sqlx::query!(
            "UPDATE workspace.session_registrations r SET released_at = now()
             FROM sessions.sessions s WHERE s.id = r.session_id AND s.session_uuid = $1 AND r.released_at IS NULL",
            session
        ).execute(self.pool()).await?;
        Ok(())
    }

    pub async fn ensure_runtime_session(&self, session: Uuid) -> Result<i64, PersistError> {
        self.ensure_runtime_session_with_limits(session, &ObservationLimits::default())
            .await
    }

    pub async fn ensure_runtime_session_with_limits(
        &self,
        session: Uuid,
        limits: &ObservationLimits,
    ) -> Result<i64, PersistError> {
        if let Some(id) = sqlx::query_scalar!(
            "SELECT i.session_id FROM runtime.session_identities i
             JOIN sessions.sessions s ON s.id = i.session_id WHERE s.session_uuid = $1",
            session
        )
        .fetch_optional(self.pool())
        .await?
        {
            return Ok(id);
        }
        let mut tx = self.pool().begin().await?;
        lock(&mut tx).await?;
        let id =
            id_resolvers::resolve_session_id(&mut *tx, &session.to_string(), None, None).await?;
        sqlx::query!("INSERT INTO runtime.session_identities (session_id) VALUES ($1) ON CONFLICT DO NOTHING", id)
            .execute(&mut *tx).await?;
        admit(&mut tx, limits).await?;
        tx.commit().await?;
        Ok(id)
    }
}

#[derive(Debug, Clone)]
pub struct ObservationLimits {
    pub max_rows: i64,
    pub max_bytes: i64,
    pub max_session_rows: i64,
    pub max_session_bytes: i64,
    pub max_record_bytes: usize,
}

impl Default for ObservationLimits {
    fn default() -> Self {
        Self {
            max_rows: 250_000,
            max_bytes: 128 * 1024 * 1024,
            max_session_rows: 20_000,
            max_session_bytes: 16 * 1024 * 1024,
            max_record_bytes: 16 * 1024,
        }
    }
}
