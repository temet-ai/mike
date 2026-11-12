use chrono::{DateTime, Utc};
use sqlx::PgConnection;

use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RuntimeRetentionOutcome {
    pub receipts_removed: u64,
    pub identities_removed: u64,
    pub calls_removed: u64,
    pub items_removed: u64,
    pub notification_targets_removed: u64,
    pub diagnostics_expired: u64,
    pub diagnostics_removed: u64,
    pub legacy_removed: u64,
}

impl PersistHandle {
    pub async fn sweep_runtime_observations(
        &self,
        now: DateTime<Utc>,
        batch: i64,
    ) -> Result<RuntimeRetentionOutcome, PersistError> {
        let batch = batch.clamp(1, 5000);
        let mut tx = self.pool().begin().await?;
        sqlx::query!("SELECT kind FROM runtime.storage_accounting ORDER BY kind FOR UPDATE")
            .fetch_all(&mut *tx)
            .await?;
        let identities_removed = delete_identities(&mut tx, now, batch).await?;
        let receipts_removed = delete_receipts(&mut tx, now, batch).await?;
        let calls_removed = delete_calls(&mut tx, now, batch).await?;
        let items_removed = delete_items(&mut tx, now, batch).await?;
        let notification_targets_removed = delete_notification_targets(&mut tx, now, batch).await?;
        let diagnostics_expired = expire_diagnostics(&mut tx, now, batch).await?;
        let diagnostics_removed = delete_diagnostics(&mut tx, now, batch).await?;
        tx.commit().await?;
        let legacy_removed = self.sweep_legacy_runtime_observations(now, batch).await?;
        Ok(RuntimeRetentionOutcome {
            identities_removed,
            receipts_removed,
            calls_removed,
            items_removed,
            diagnostics_expired,
            diagnostics_removed,
            legacy_removed,
            notification_targets_removed,
        })
    }

    async fn sweep_legacy_runtime_observations(
        &self,
        now: DateTime<Utc>,
        batch: i64,
    ) -> Result<u64, PersistError> {
        let mut tx = self.pool().begin().await?;
        let mut removed = 0;
        removed += legacy_01_to_03(&mut tx, now, batch).await?;
        removed += legacy_04_to_05(&mut tx, now, batch).await?;
        retire_stale_generations(&mut tx, now).await?;
        removed += legacy_06_to_08(&mut tx, now, batch).await?;
        removed += legacy_09_to_11(&mut tx, now, batch).await?;
        removed += sweep_empty_namespace_state(&mut tx, now, batch).await?;
        tx.commit().await?;
        Ok(removed)
    }
}

async fn delete_identities(
    connection: &mut PgConnection,
    now: DateTime<Utc>,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query_file!(
        "src/store/persistence/runtime/retention/identities.sql",
        now,
        batch
    )
    .execute(&mut *connection)
    .await?
    .rows_affected())
}

async fn delete_receipts(
    connection: &mut PgConnection,
    now: DateTime<Utc>,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!(
        "DELETE FROM runtime.observation_receipts WHERE (session_id, observation_id) IN
             (SELECT session_id, observation_id FROM runtime.observation_receipts
              WHERE expires_at <= $1 ORDER BY expires_at LIMIT $2)",
        now,
        batch
    )
    .execute(&mut *connection)
    .await?
    .rows_affected())
}

async fn delete_calls(
    connection: &mut PgConnection,
    now: DateTime<Utc>,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!(
            "DELETE FROM runtime.model_calls WHERE id IN
             (SELECT id FROM runtime.model_calls WHERE observed_at < $1::timestamptz - interval '90 days'
              ORDER BY observed_at LIMIT $2)", now, batch
        ).execute(&mut *connection).await?.rows_affected())
}

async fn delete_items(
    connection: &mut PgConnection,
    now: DateTime<Utc>,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!(
            "DELETE FROM runtime.session_items WHERE id IN
             (SELECT id FROM runtime.session_items WHERE observed_at < $1::timestamptz - interval '90 days'
              ORDER BY observed_at LIMIT $2)", now, batch
        ).execute(&mut *connection).await?.rows_affected())
}

async fn delete_notification_targets(
    connection: &mut PgConnection,
    now: DateTime<Utc>,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!(
            "DELETE FROM runtime.notification_targets WHERE session_id IN
             (SELECT session_id FROM runtime.notification_targets
              WHERE verified_at < $1::timestamptz - interval '7 days' ORDER BY verified_at LIMIT $2)", now, batch
        ).execute(&mut *connection).await?.rows_affected())
}

async fn expire_diagnostics(
    connection: &mut PgConnection,
    now: DateTime<Utc>,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!(
            "UPDATE runtime.responses_exchanges SET request_body = NULL, forwarded_body = NULL,
                 response_body = NULL, output_items = '[]'::jsonb, evidence_status = 'expired'
             WHERE id IN (SELECT id FROM runtime.responses_exchanges
                 WHERE evidence_status = 'retained' AND expires_at <= $1 ORDER BY expires_at LIMIT $2)",
            now, batch
        ).execute(&mut *connection).await?.rows_affected())
}

async fn delete_diagnostics(
    connection: &mut PgConnection,
    now: DateTime<Utc>,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query!(
        "DELETE FROM runtime.responses_exchanges WHERE id IN
             (SELECT id FROM runtime.responses_exchanges WHERE metadata_expires_at <= $1
              ORDER BY metadata_expires_at LIMIT $2)",
        now,
        batch
    )
    .execute(&mut *connection)
    .await?
    .rows_affected())
}

async fn legacy_01_to_03(
    connection: &mut PgConnection,
    now: DateTime<Utc>,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    let mut removed = 0;
    removed += sqlx::query_file!(
        "src/store/persistence/runtime/retention/legacy_01.sql",
        now,
        batch
    )
    .execute(&mut *connection)
    .await?
    .rows_affected();
    removed += sqlx::query_file!(
        "src/store/persistence/runtime/retention/legacy_02.sql",
        now,
        batch
    )
    .execute(&mut *connection)
    .await?
    .rows_affected();
    removed += sqlx::query_file!(
        "src/store/persistence/runtime/retention/legacy_03.sql",
        now,
        batch
    )
    .execute(&mut *connection)
    .await?
    .rows_affected();
    Ok(removed)
}

async fn legacy_04_to_05(
    connection: &mut PgConnection,
    now: DateTime<Utc>,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    let mut removed = 0;
    removed += sqlx::query_file!(
        "src/store/persistence/runtime/retention/legacy_04.sql",
        now,
        batch
    )
    .execute(&mut *connection)
    .await?
    .rows_affected();
    removed += sqlx::query_file!(
        "src/store/persistence/runtime/retention/legacy_05.sql",
        now,
        batch
    )
    .execute(&mut *connection)
    .await?
    .rows_affected();
    Ok(removed)
}

async fn legacy_06_to_08(
    connection: &mut PgConnection,
    now: DateTime<Utc>,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    let mut removed = 0;
    removed += sqlx::query_file!(
        "src/store/persistence/runtime/retention/legacy_06.sql",
        now,
        batch
    )
    .execute(&mut *connection)
    .await?
    .rows_affected();
    removed += sqlx::query_file!(
        "src/store/persistence/runtime/retention/legacy_07.sql",
        now,
        batch
    )
    .execute(&mut *connection)
    .await?
    .rows_affected();
    removed += sqlx::query_file!(
        "src/store/persistence/runtime/retention/legacy_08.sql",
        now,
        batch
    )
    .execute(&mut *connection)
    .await?
    .rows_affected();
    Ok(removed)
}

async fn legacy_09_to_11(
    connection: &mut PgConnection,
    now: DateTime<Utc>,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    let mut removed = 0;
    removed += sqlx::query_file!(
        "src/store/persistence/runtime/retention/legacy_09.sql",
        now,
        batch
    )
    .execute(&mut *connection)
    .await?
    .rows_affected();
    removed += sqlx::query_file!(
        "src/store/persistence/runtime/retention/legacy_10.sql",
        now,
        batch
    )
    .execute(&mut *connection)
    .await?
    .rows_affected();
    removed += sqlx::query_file!(
        "src/store/persistence/runtime/retention/legacy_11.sql",
        now,
        batch
    )
    .execute(&mut *connection)
    .await?
    .rows_affected();
    Ok(removed)
}

async fn retire_stale_generations(
    connection: &mut PgConnection,
    now: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "UPDATE runtime.codex_projection_generations g SET state = 'retired'
         WHERE g.state IN ('building', 'active') AND g.created_at < $1::timestamptz - interval '90 days'
           AND NOT EXISTS (SELECT 1 FROM runtime.events e WHERE e.namespace_id = g.namespace_id
                           AND e.created_at >= $1::timestamptz - interval '90 days')", now
    ).execute(&mut *connection).await?;
    sqlx::query!(
        "UPDATE runtime.codex_projection_heads h SET active_generation_id = NULL
         FROM runtime.codex_projection_generations g
         WHERE h.active_generation_id = g.generation_id AND g.state IN ('retired', 'failed')"
    )
    .execute(connection)
    .await?;
    Ok(())
}

async fn sweep_empty_namespace_state(
    connection: &mut PgConnection,
    now: DateTime<Utc>,
    batch: i64,
) -> Result<u64, sqlx::Error> {
    sqlx::query!(
        "DELETE FROM runtime.codex_projection_heads WHERE namespace_id IN
         (SELECT h.namespace_id FROM runtime.codex_projection_heads h
          JOIN runtime.namespaces n ON n.id = h.namespace_id
          WHERE h.active_generation_id IS NULL AND n.created_at < $1::timestamptz - interval '90 days'
            AND NOT EXISTS (SELECT 1 FROM runtime.codex_projection_generations g WHERE g.namespace_id = h.namespace_id)
          LIMIT $2)", now, batch
    ).execute(&mut *connection).await?;
    sqlx::query!(
        "DELETE FROM runtime.codex_intake_state WHERE namespace_id IN
         (SELECT i.namespace_id FROM runtime.codex_intake_state i
          JOIN runtime.namespaces n ON n.id = i.namespace_id
          WHERE n.created_at < $1::timestamptz - interval '90 days'
            AND NOT EXISTS (SELECT 1 FROM runtime.events e WHERE e.namespace_id = i.namespace_id)
          LIMIT $2)",
        now,
        batch
    )
    .execute(&mut *connection)
    .await?;
    Ok(sqlx::query_file!(
        "src/store/persistence/runtime/retention/namespaces.sql",
        now,
        batch
    )
    .execute(connection)
    .await?
    .rows_affected())
}
