pub mod acceptance_criteria;
pub(crate) mod agent_dispatch_reads;
pub mod agents;
pub mod ask_ledger_backfill;
pub mod assemble;
pub(crate) mod capture_classification;
pub(crate) mod capture_support;
pub mod change_log;
pub mod change_record;
pub(crate) mod composition_usage;
pub mod compositions;
#[cfg(test)]
mod connect_existing_tests;
pub mod consolidation_audit;
pub(crate) mod consolidation_reads;
pub(crate) mod content_search;
pub mod cross_session;
pub(crate) mod db_cost_reads;
pub(crate) mod db_maintenance_reads;
pub(crate) mod dispatch_instances;
#[cfg(test)]
mod dispatch_instances_tests;
pub mod dossier_entries;
pub mod dossier_parity_reads;
pub mod dossier_view;
pub(crate) mod error;
pub mod eval_bundles;
pub mod eval_labels;
pub mod eval_metrics;
pub mod eval_pointer;
pub mod eval_rejudge;
pub mod eval_runs;
pub mod feature_toggles;
#[cfg(test)]
mod feature_toggles_db_tests;
mod feed_memo;
pub(crate) mod file_write_summaries;
pub(crate) mod file_writes;
pub mod filter_rules;
pub mod fleet;
pub mod fleet_correlation;
pub(crate) mod flow_capture_admission;
mod id_cache;
#[cfg(test)]
mod id_cache_db_tests;
pub mod id_resolvers;
pub mod improvement;
pub mod judge;
pub mod judge_digest;
pub(crate) mod judge_ownership;
pub(crate) mod keeper_earlier;
#[cfg(test)]
mod keeper_earlier_tests;
pub mod keyset;
pub mod launch;
pub mod lesson_injections;
pub(crate) mod lesson_pair_candidates;
pub mod lesson_relations;
pub mod lessons;
pub(crate) mod listings;
pub mod llm_calls;
pub mod llm_role_health;
pub(crate) mod machine_merge;
pub mod mailbox;
pub mod memory_ask_ledger;
pub mod messages;
pub mod meta;
pub mod migrations;
pub mod model_usage;
#[cfg(test)]
mod opened_by_readers_tests;
pub mod operator_turns;
pub mod origin_backfill;
pub(crate) mod pool;
pub(crate) mod pool_classes;
pub mod project_knowledge;
pub(crate) mod project_merge;
pub(crate) mod project_pin_ttl;
pub mod prompt_classification;
pub mod prompt_stream;
pub mod questions;
mod recall_embedding;
#[cfg(test)]
mod recall_embedding_tests;
pub(crate) mod recall_reads;
pub mod reconstruct;
pub mod rehydrate;
pub mod remote_control;
pub(crate) mod request_body_compaction;
pub(crate) mod request_integrity_backfill;
pub(crate) mod request_lifecycle_close;
pub(crate) mod request_snapshot;
pub(crate) mod request_snapshot_prep;
#[cfg(test)]
mod request_snapshot_tests_diagnostics;
#[cfg(test)]
mod request_snapshot_tests_reconnect;
#[cfg(test)]
mod request_snapshot_tests_summary;
pub(crate) mod request_write;
pub(crate) mod request_write_aux;
#[cfg(test)]
mod request_write_capture_integrity_tests;
pub(crate) mod request_write_message_bridge;
pub(crate) mod request_write_orchestrator;
#[cfg(test)]
mod request_write_tests_author_kind;
#[cfg(test)]
mod request_write_tests_dedup;
#[cfg(test)]
mod request_write_tests_patches;
#[cfg(test)]
mod request_write_tests_session_prompt_lane;
#[cfg(test)]
mod request_write_tests_system_prompt_id;
pub(crate) mod request_write_tool_io;
pub(crate) mod request_write_worker_producer;
pub(crate) mod response_intercepts;
pub mod retention;
pub mod runtime;
pub(crate) mod scheduled_job_runs;
pub(crate) mod search_events;
pub(crate) mod search_grades;
pub mod seat_memory;
pub mod semantic;
pub mod session_action_list;
pub mod session_analyzer_overrides;
pub(crate) mod session_delivery;
pub(crate) mod session_diagnostics_reads;
pub(crate) mod session_embeddings;
pub mod session_end;
pub mod session_flow;
pub mod session_goals;
mod session_ids;
mod session_latest;
#[cfg(test)]
mod session_latest_tests;
mod session_locks;
pub mod session_notes;
pub(crate) mod session_overview;
pub(crate) mod session_projects;
pub(crate) mod session_prompt_lane_backfill;
pub(crate) mod session_prompt_repair;
pub(crate) mod session_request_reads;
pub mod settings;
pub mod startup_health;
pub mod steering;
pub(crate) mod steering_backfill;
pub mod steering_evaluations;
pub mod steering_facts;
pub(crate) mod system_prompt_backfill;
#[cfg(any(test, feature = "test-seams"))]
pub mod test_support;
#[cfg(test)]
mod thread_card_generations_tests;
pub(crate) mod time_spent;
pub mod tool_call_reads;
pub mod tool_errors;
pub(crate) mod tool_use_stats;
pub mod tool_uses;
pub mod tools;
pub mod turn_facts;
pub(crate) mod turn_lifecycle;
pub(crate) mod turn_origin_repair;
pub(crate) mod turn_tool_call_reads;
pub mod turn_verdicts;
pub mod verifier_replay;
pub(crate) mod volatile;
pub(crate) mod work_item_attention;
#[cfg(test)]
mod work_item_attention_tests;
pub(crate) mod work_items;
pub(crate) mod work_items_backfill;
pub(crate) mod work_items_ledger;
pub(crate) mod work_items_reconcile;
pub mod work_threads;
pub mod worker_leases;
pub(crate) mod workflow_judge_reads;
pub mod workspace;

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Utc};
use sqlx::PgPool;
use tokio::sync::RwLock;

pub use error::PersistError;
pub use fleet::{FleetNoteRow, FleetRow, FleetSeatRow, NewFleet};
pub use judge::{NeedsJudgmentQuery, SessionForJudgment, fetch_sessions_needing_judgment};
pub use lesson_relations::{LessonRelationDraft, RelationDecisionOutcome};
pub use operator_turns::JudgeCandidateRow;
pub(crate) use rehydrate::SessionSummaryRow;
pub use session_embeddings::SessionEmbeddingWrite;

pub(crate) use agent_dispatch_reads::AgentDispatchFilter;
pub(crate) use capture_classification::{
    extract_system_blocks_from_body, session_prompt_content_sha256,
};
pub(crate) use capture_support::StreamBlockRow;
#[cfg(any(test, feature = "test-seams"))]
pub use id_resolvers::resolve_session_id;
pub(crate) use recall_reads::RankedTurnRow;
pub use request_write::IdMode;
pub(crate) use request_write::{RequestInsertOutcome, RowInsert};
pub(crate) use tool_call_reads::{BatchToolResultRow, ToolUseRow};
pub(crate) use turn_tool_call_reads::TurnToolCallContentRow;

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
    pool: PgPool,
    pools: pool_classes::PoolClasses,
    active_target: ActiveDbTarget,
    main_lane_captured: Arc<RwLock<HashSet<i64>>>,
    judge_ownership_capacity: judge_ownership::JudgeOwnershipCapacity,
    llm_leases: worker_leases::LeaseMemo,
    id_cache: id_cache::IdCache,
    open_changes_memo: change_record::open_memo::OpenChangesMemo,
    feed_memos: feed_memo::FeedMemos,
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

    pub(crate) async fn connect_with_timeout(
        database_url: &str,
        acquire_timeout: Duration,
    ) -> Result<Self, PersistError> {
        Self::connect_with_policy(database_url, acquire_timeout, SchemaPolicy::Migrate).await
    }

    pub(crate) async fn connect_with_policy(
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

    pub(crate) fn into_local_fallback(mut self) -> Self {
        self.active_target = ActiveDbTarget::LocalFallback;
        self
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub(crate) fn pool_connection_stats(&self) -> (u32, usize) {
        self.class_pools()
            .into_iter()
            .fold((0, 0), |(size, idle), (_, pool)| {
                (size + pool.size(), idle + pool.num_idle())
            })
    }

    pub(crate) async fn ping(&self) -> Result<(), sqlx::Error> {
        self.pool.acquire().await?;
        Ok(())
    }

    pub(crate) async fn latest_capture_write_at(
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

    pub(crate) async fn preflight(&self) -> Result<(), PersistError> {
        if !self.schema_exists().await? {
            return Err(PersistError::DataDir(
                "mike schema missing after migrations — the migration set may have been skipped or the database user lacks schema-create privileges".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
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
