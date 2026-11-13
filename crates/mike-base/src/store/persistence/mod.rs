pub mod agent_dispatch_reads;
pub mod agents;
pub mod assemble;
pub mod capture_classification;
pub mod capture_retry;
pub mod capture_support;
pub mod change_log;
pub mod change_record;
pub mod composition_usage;
#[cfg(test)]
pub mod connect_existing_tests;
pub mod content_search;
pub mod dispatch_instances;
#[cfg(test)]
pub mod dispatch_instances_tests;
pub mod error;
pub mod feature_toggles;
pub mod feed_memo;
pub mod file_writes;
pub mod flow_capture_admission;
pub mod id_cache;
pub mod id_resolvers;
pub mod judge_ownership;
pub mod keyset;
pub mod llm_calls;
pub mod machine_merge;
pub mod memo_slot;
pub mod messages;
pub mod meta;
pub mod migrations;
pub mod model_usage;
pub mod new_session_hook;
pub mod operator_turns;
pub mod origin_backfill;
pub mod pool;
pub mod pool_classes;
pub mod prompt_classification;
pub mod reconstruct;
pub mod rehydrate;
pub mod request_body_compaction;
pub mod request_integrity_backfill;
pub mod request_lifecycle_close;
pub mod request_snapshot;
pub mod request_snapshot_prep;
#[cfg(test)]
pub mod request_snapshot_tests_diagnostics;
#[cfg(test)]
pub mod request_snapshot_tests_reconnect;
#[cfg(test)]
pub mod request_snapshot_tests_summary;
pub mod request_write;
pub mod request_write_aux;
#[cfg(test)]
pub mod request_write_capture_integrity_tests;
pub mod request_write_message_bridge;
pub mod request_write_orchestrator;
#[cfg(test)]
pub mod request_write_tests_author_kind;
#[cfg(test)]
pub mod request_write_tests_dedup;
#[cfg(test)]
pub mod request_write_tests_patches;
#[cfg(test)]
pub mod request_write_tests_session_prompt_lane;
#[cfg(test)]
pub mod request_write_tests_system_prompt_id;
pub mod request_write_tool_io;
pub mod request_write_worker_producer;
pub mod retention;
pub mod runtime;
pub mod session_analyzer_overrides;
pub mod session_diagnostics_reads;
pub mod session_ids;
pub mod session_latest;
#[cfg(test)]
pub mod session_latest_tests;
pub mod session_locks;
pub mod session_overview;
pub mod session_projects;
pub mod session_prompt_lane_backfill;
pub mod session_prompt_repair;
pub mod session_request_reads;
pub mod session_titles;
pub mod settings;
pub mod startup_health;
pub mod system_prompt_backfill;
#[cfg(any(test, feature = "test-seams"))]
pub mod test_support;
pub mod text_sanitize;
pub mod tool_errors;
pub mod tool_use_stats;
pub mod tools;
pub mod turn_lifecycle;
pub mod turn_origin_repair;
pub mod work_items;
pub mod work_items_backfill;
pub mod worker_leases;
pub mod workspace;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tokio::sync::RwLock;

pub use error::PersistError;
pub use operator_turns::JudgeCandidateRow;
pub use rehydrate::SessionSummaryRow;

pub use agent_dispatch_reads::AgentDispatchFilter;
pub use capture_classification::{
    extract_system_blocks_from_body, session_prompt_content_sha256,
};
pub use capture_support::StreamBlockRow;
#[cfg(any(test, feature = "test-seams"))]
pub use id_resolvers::resolve_session_id;
pub use request_write::IdMode;
pub use request_write::{RequestInsertOutcome, RowInsert};

/// Whether connecting may apply this binary's embedded migrations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SchemaPolicy {
    /// Apply pending migrations. Only `serve` connects this way.
    Migrate,
    /// Apply nothing; refuse unless the ledger already matches the binary.
    RequireCurrent,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ActiveDbTarget {
    Remote,
    LocalFallback,
}

pub const SENTINEL_PROJECT_PATH: &str = "(no project)";

#[derive(Debug, Clone)]
pub struct PersistHandle {
    pub pool: PgPool,
    pools: pool_classes::PoolClasses,
    active_target: ActiveDbTarget,
    main_lane_captured: Arc<RwLock<HashSet<i64>>>,
    judge_ownership_capacity: judge_ownership::JudgeOwnershipCapacity,
    llm_leases: worker_leases::LeaseMemo,
    pub id_cache: id_cache::IdCache,
    pub feed_memos: feed_memo::FeedMemos,
    memos: memo_slot::MemoSlot,
}

impl PersistHandle {
    pub async fn connect(database_url: &str) -> Result<Self, PersistError> {
        Self::connect_with_timeout(database_url, pool::DEFAULT_ACQUIRE_TIMEOUT).await
    }

    /// Connects without migrating. Refuses when the database's migration ledger
    /// is behind (or incompatible with) this binary's embedded migrations, so a
    /// branch or worktree binary never applies unmerged migrations to a shared
    /// database. Only `serve` migrates (#359).
    pub async fn connect_existing(database_url: &str) -> Result<Self, PersistError> {
        Self::connect_with_policy(
            database_url,
            pool::DEFAULT_ACQUIRE_TIMEOUT,
            SchemaPolicy::RequireCurrent,
        )
        .await
    }

    pub async fn connect_with_timeout(
        database_url: &str,
        acquire_timeout: Duration,
    ) -> Result<Self, PersistError> {
        Self::connect_with_policy(database_url, acquire_timeout, SchemaPolicy::Migrate).await
    }

    pub async fn connect_with_policy(
        database_url: &str,
        acquire_timeout: Duration,
        policy: SchemaPolicy,
    ) -> Result<Self, PersistError> {
        let pool = pool::build_pool_with_timeout(database_url, acquire_timeout).await?;
        match policy {
            SchemaPolicy::Migrate => migrations::run_patient(database_url).await?,
            SchemaPolicy::RequireCurrent => migrations::require_current(&pool).await?,
        }
        Ok(Self::from_pools(pool_classes::PoolClasses::single(pool)))
    }

    pub async fn connect_read_only(database_url: &str) -> Result<Self, PersistError> {
        let pool = pool::build_read_only_pool(database_url).await?;
        Ok(Self::from_pools(pool_classes::PoolClasses::single(pool)))
    }

    pub fn active_target(&self) -> ActiveDbTarget {
        self.active_target
    }

    pub fn into_local_fallback(mut self) -> Self {
        self.active_target = ActiveDbTarget::LocalFallback;
        self
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub fn pool_connection_stats(&self) -> (u32, usize) {
        self.class_pools()
            .into_iter()
            .fold((0, 0), |(size, idle), (_, pool)| {
                (size + pool.size(), idle + pool.num_idle())
            })
    }

    pub async fn ping(&self) -> Result<(), sqlx::Error> {
        self.pool.acquire().await?;
        Ok(())
    }

    pub async fn latest_capture_write_at(
        &self,
    ) -> Result<Option<DateTime<Utc>>, PersistError> {
        crate::metrics::time_db("latest_capture_write_at", async move {
            let ts = sqlx::query_scalar!("SELECT MAX(created_at) FROM requests.requests")
                .fetch_one(&self.pool)
                .await?;
            Ok(ts)
        })
        .await
    }

    pub async fn preflight(&self) -> Result<(), PersistError> {
        if !self.schema_exists().await? {
            return Err(PersistError::DataDir(
                "mike schema missing after migrations — the migration set may have been skipped or the database user lacks schema-create privileges".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    use crate::store::persistence::test_support as test_util;

    #[tokio::test]
    async fn connect_fails_on_bad_url() -> anyhow::Result<()> {
        let bad_url = "postgres://nobody:nobody@127.0.0.1:1/claude_proxy_never";
        match PersistHandle::connect_with_timeout(bad_url, Duration::from_millis(200)).await {
            Err(PersistError::Connect { .. }) => Ok(()),
            Err(other) => anyhow::bail!("expected Connect, got {other:?}"),
            Ok(_) => anyhow::bail!("expected error, got handle"),
        }
    }

    #[tokio::test]
    async fn connect_runs_migrations_and_is_idempotent() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let first = PersistHandle::connect(&db.database_url).await?;
        assert!(
            first.schema_exists().await?,
            "mike schema must exist after migrations"
        );

        const FORGOTTEN_VERSION: i64 = 20260908000001;
        const DROP_VERSION_COLUMNS: &str = "ALTER TABLE system_prompts.session_prompts DROP COLUMN cc_release, DROP COLUMN cc_build";
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version = $1")
            .bind(FORGOTTEN_VERSION)
            .execute(first.pool())
            .await?;
        sqlx::query("DROP INDEX IF EXISTS system_prompts.idx_session_prompts_lane_persona_version")
            .execute(first.pool())
            .await?;
        sqlx::query(DROP_VERSION_COLUMNS)
            .execute(first.pool())
            .await?;

        let second = PersistHandle::connect(&db.database_url).await?;
        let reapplied: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM _sqlx_migrations WHERE version = $1)")
                .bind(FORGOTTEN_VERSION)
                .fetch_one(second.pool())
                .await?;
        assert!(
            reapplied,
            "connect must re-apply a migration missing from the ledger"
        );
        let column_back: bool = sqlx::query_scalar(
            "SELECT EXISTS (SELECT 1 FROM information_schema.columns \
             WHERE table_schema = 'system_prompts' AND table_name = 'session_prompts' \
             AND column_name = 'cc_release')",
        )
        .fetch_one(second.pool())
        .await?;
        assert!(
            column_back,
            "the re-applied migration must recreate cc_release"
        );
        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn connect_read_only_runs_no_migrations() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        const FORGOTTEN_VERSION: i64 = 20260908000001;
        let admin_pool = sqlx::PgPool::connect(&db.database_url).await?;
        sqlx::query("DELETE FROM _sqlx_migrations WHERE version = $1")
            .bind(FORGOTTEN_VERSION)
            .execute(&admin_pool)
            .await?;
        admin_pool.close().await;

        let handle = PersistHandle::connect_read_only(&db.database_url).await?;
        let reapplied: bool =
            sqlx::query_scalar("SELECT EXISTS (SELECT 1 FROM _sqlx_migrations WHERE version = $1)")
                .bind(FORGOTTEN_VERSION)
                .fetch_one(handle.pool())
                .await?;
        assert!(
            !reapplied,
            "connect_read_only must not re-apply a migration missing from the ledger"
        );
        handle.pool().close().await;
        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn connect_read_only_rejects_writes() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect_read_only(&db.database_url).await?;
        let err = sqlx::query("CREATE TABLE read_only_probe (id int)")
            .execute(handle.pool())
            .await
            .err()
            .ok_or_else(|| anyhow::anyhow!("a write through the read-only pool succeeded"))?;
        assert!(
            err.to_string().contains("read-only transaction"),
            "expected a read-only error, got {err}"
        );
        handle.pool().close().await;
        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn preflight_succeeds() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;
        handle.preflight().await?;
        db.cleanup().await?;
        Ok(())
    }

    #[tokio::test]
    async fn latest_capture_write_at_tracks_newest_request_row() -> anyhow::Result<()> {
        let Some(db) = test_util::acquire_ephemeral_db().await? else {
            return Ok(());
        };
        let handle = PersistHandle::connect(&db.database_url).await?;

        assert!(
            handle.latest_capture_write_at().await?.is_none(),
            "an empty requests.requests table must report None, not a fault"
        );

        let project_id = test_util::seed_project(&handle.pool, "capture-staleness-proj").await?;
        let session_a = uuid::Uuid::new_v4().to_string();
        let session_b = uuid::Uuid::new_v4().to_string();
        test_util::seed_two_sessions(&handle.pool, &session_a, &session_b, project_id).await?;
        test_util::seed_session_turns(&handle.pool, &session_a, project_id, 3).await?;

        let latest = handle
            .latest_capture_write_at()
            .await?
            .ok_or_else(|| anyhow::anyhow!("expected a timestamp once rows exist"))?;
        let (max_created_at,): (chrono::DateTime<chrono::Utc>,) =
            sqlx::query_as("SELECT MAX(created_at) FROM requests.requests")
                .fetch_one(&handle.pool)
                .await?;
        assert_eq!(
            latest, max_created_at,
            "latest_capture_write_at must equal the true MAX(created_at)"
        );

        db.cleanup().await?;
        Ok(())
    }
}
