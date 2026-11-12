use sqlx::{PgConnection, types::Json};
use uuid::Uuid;

use super::{ObservationLimits, RuntimeWriteOutcome, identity_budget, invalid, same_or_conflict};
use crate::runtime::{ProviderThreadRef, RuntimeNamespace, RuntimeThreadBinding};
use crate::store::persistence::{PersistError, PersistHandle};

struct ThreadLineage<'a> {
    parent_namespace: Option<Uuid>,
    parent_thread: Option<&'a str>,
    fork_namespace: Option<Uuid>,
    fork_thread: Option<&'a str>,
}

impl<'a> ThreadLineage<'a> {
    fn of(binding: &'a RuntimeThreadBinding) -> Self {
        let (parent_namespace, parent_thread) = provider_parts(binding.native_parent.as_ref());
        let (fork_namespace, fork_thread) = provider_parts(binding.forked_from.as_ref());
        Self {
            parent_namespace,
            parent_thread,
            fork_namespace,
            fork_thread,
        }
    }
}

fn provider_parts(thread: Option<&ProviderThreadRef>) -> (Option<Uuid>, Option<&str>) {
    thread.map_or((None, None), |value| {
        (Some(value.namespace_id), Some(value.thread_id.as_str()))
    })
}

impl PersistHandle {
    pub async fn resolve_unique_codex_session(
        &self,
        thread: &str,
    ) -> Result<Option<Uuid>, PersistError> {
        if thread.is_empty() || thread.len() > 512 {
            return Ok(None);
        }
        let sessions = sqlx::query_scalar!(
            "SELECT s.session_uuid FROM runtime.threads t
             JOIN runtime.namespaces n ON n.id = t.namespace_id
             JOIN sessions.sessions s ON s.id = t.session_id
             WHERE n.runtime_kind = 'codex' AND t.provider_thread_id = $1 LIMIT 2",
            thread
        )
        .fetch_all(self.pool())
        .await?;
        Ok(if sessions.len() == 1 {
            sessions.first().copied()
        } else {
            None
        })
    }

    pub async fn register_runtime_namespace(
        &self,
        namespace: &RuntimeNamespace,
    ) -> Result<RuntimeWriteOutcome, PersistError> {
        namespace.validate().map_err(invalid)?;
        if serde_json::to_vec(namespace)?.len() > 16 * 1024 {
            return Err(PersistError::DataInvariant(
                "runtime namespace exceeds 16 KiB".into(),
            ));
        }
        let mut tx = self.pool().begin().await?;
        identity_budget::lock(&mut tx).await?;
        let inserted = sqlx::query_scalar!(
            "INSERT INTO runtime.namespaces (id, runtime_kind, durable_key, identity)
             VALUES ($1, $2, $3, $4) ON CONFLICT DO NOTHING RETURNING id",
            namespace.id,
            namespace.runtime.as_str(),
            namespace.durable_key,
            serde_json::to_value(namespace)?,
        )
        .fetch_optional(&mut *tx)
        .await?;
        if inserted.is_some() {
            let count =
                sqlx::query_scalar!(r#"SELECT count(*) AS "count!" FROM runtime.namespaces"#)
                    .fetch_one(&mut *tx)
                    .await?;
            if count > 1024 {
                return Err(PersistError::DataInvariant(
                    "runtime namespace row budget exhausted".into(),
                ));
            }
            identity_budget::admit(&mut tx, &ObservationLimits::default()).await?;
            tx.commit().await?;
            return Ok(RuntimeWriteOutcome::Inserted);
        }
        tx.rollback().await?;
        same_or_conflict(
            self.runtime_namespace(namespace.id).await?,
            namespace,
            "namespace",
        )
    }

    pub async fn runtime_namespace(
        &self,
        id: Uuid,
    ) -> Result<Option<RuntimeNamespace>, PersistError> {
        let row = sqlx::query_scalar!(
            r#"SELECT identity AS "identity!: Json<RuntimeNamespace>"
                 FROM runtime.namespaces WHERE id = $1"#,
            id,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| row.0))
    }

    pub async fn bind_runtime_thread(
        &self,
        binding: &RuntimeThreadBinding,
    ) -> Result<RuntimeWriteOutcome, PersistError> {
        binding.validate().map_err(invalid)?;
        let identity = serde_json::to_value(binding)?;
        if serde_json::to_vec(binding)?.len() > 16 * 1024 {
            return Err(PersistError::DataInvariant(
                "runtime thread identity exceeds 16 KiB".into(),
            ));
        }
        let mut tx = self.pool().begin().await?;
        identity_budget::lock(&mut tx).await?;
        let lineage = ThreadLineage::of(binding);
        let inserted = match binding.owning_seat_id {
            Some(owner) => {
                Self::insert_owned_thread(&mut tx, binding, owner, &lineage, identity).await?
            }
            None => Self::insert_unowned_thread(&mut tx, binding, &lineage, identity).await?,
        };
        if inserted {
            identity_budget::admit(&mut tx, &ObservationLimits::default()).await?;
            tx.commit().await?;
            return Ok(RuntimeWriteOutcome::Inserted);
        }
        tx.rollback().await?;
        same_or_conflict(
            self.runtime_thread(&binding.provider).await?,
            binding,
            "thread",
        )
    }

    async fn insert_owned_thread(
        connection: &mut PgConnection,
        binding: &RuntimeThreadBinding,
        owner: Uuid,
        lineage: &ThreadLineage<'_>,
        identity: serde_json::Value,
    ) -> Result<bool, PersistError> {
        let inserted = sqlx::query_file_scalar!(
            "src/store/persistence/runtime/bind_thread_owned.sql",
            binding.mike_session_id,
            binding.provider.namespace_id,
            binding.provider.thread_id,
            binding.provider_session_id,
            owner,
            lineage.parent_namespace,
            lineage.parent_thread,
            lineage.fork_namespace,
            lineage.fork_thread,
            identity,
        )
        .fetch_optional(connection)
        .await?;
        Ok(inserted.is_some())
    }

    async fn insert_unowned_thread(
        connection: &mut PgConnection,
        binding: &RuntimeThreadBinding,
        lineage: &ThreadLineage<'_>,
        identity: serde_json::Value,
    ) -> Result<bool, PersistError> {
        let inserted = sqlx::query_file_scalar!(
            "src/store/persistence/runtime/bind_thread_unowned.sql",
            binding.mike_session_id,
            binding.provider.namespace_id,
            binding.provider.thread_id,
            binding.provider_session_id,
            lineage.parent_namespace,
            lineage.parent_thread,
            lineage.fork_namespace,
            lineage.fork_thread,
            identity,
        )
        .fetch_optional(connection)
        .await?;
        Ok(inserted.is_some())
    }

    pub async fn runtime_thread(
        &self,
        provider: &ProviderThreadRef,
    ) -> Result<Option<RuntimeThreadBinding>, PersistError> {
        provider.validate().map_err(invalid)?;
        let row = sqlx::query_scalar!(
            r#"SELECT identity AS "identity!: Json<RuntimeThreadBinding>"
                 FROM runtime.threads WHERE namespace_id = $1 AND provider_thread_id = $2"#,
            provider.namespace_id,
            provider.thread_id,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| row.0))
    }

    pub async fn runtime_thread_by_session(
        &self,
        session_id: Uuid,
    ) -> Result<Option<RuntimeThreadBinding>, PersistError> {
        let row = sqlx::query_scalar!(
            r#"SELECT thread.identity AS "identity!: Json<RuntimeThreadBinding>"
                 FROM runtime.threads thread
                 JOIN sessions.sessions session ON session.id = thread.session_id
                WHERE session.session_uuid = $1"#,
            session_id,
        )
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| row.0))
    }
}
