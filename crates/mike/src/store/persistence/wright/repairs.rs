use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use crate::store::persistence::PersistError;

const CLAIM: &str = include_str!("sql/repairs_claim.sql");
const ADVANCE: &str = include_str!("sql/repairs_advance.sql");
const UNCLAIM: &str = include_str!("sql/repairs_unclaim.sql");
const WAITING: &str = include_str!("sql/repairs_waiting.sql");
const FAILED_RUNS: &str = include_str!("sql/repairs_failed_runs.sql");
const LANDED: &str = include_str!("sql/repairs_landed.sql");
const MARK_DEPLOYED: &str = include_str!("sql/repairs_mark_deployed.sql");
const MARK_HEALTHY: &str = include_str!("sql/repairs_mark_healthy.sql");
const LIST: &str = include_str!("sql/repairs_list.sql");
const ONE: &str = include_str!("sql/repairs_one.sql");
const INTERRUPTED: &str = include_str!("sql/repairs_interrupted.sql");

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub(crate) struct RepairRow {
    pub(crate) id: i64,
    pub(crate) finding_id: i64,
    pub(crate) problem_id: i64,
    pub(crate) owning_part: String,
    pub(crate) state: String,
    pub(crate) worktree: String,
    pub(crate) branch: String,
    pub(crate) branches: Vec<String>,
    pub(crate) base_sha: Option<String>,
    pub(crate) commit_sha: Option<String>,
    pub(crate) gates: i32,
    pub(crate) patch: Value,
    pub(crate) gate_exit: Option<i32>,
    pub(crate) gate_output: Option<String>,
    pub(crate) gate_output_bytes: Option<i32>,
    pub(crate) pr_url: Option<String>,
    pub(crate) merge_sha: Option<String>,
    pub(crate) reason: Option<String>,
    pub(crate) machine_uuid: Option<Uuid>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) prepared_at: Option<DateTime<Utc>>,
    pub(crate) gated_at: Option<DateTime<Utc>>,
    pub(crate) landed_at: Option<DateTime<Utc>>,
    pub(crate) deployed_at: Option<DateTime<Utc>>,
    pub(crate) healthy_at: Option<DateTime<Utc>>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub(crate) struct ClaimedRepair {
    pub(crate) id: i64,
    pub(crate) finding_id: i64,
    pub(crate) problem_id: i64,
    pub(crate) owning_part: String,
    pub(crate) worktree: String,
    pub(crate) branch: String,
    pub(crate) check_name: String,
    pub(crate) symptom: Option<String>,
    pub(crate) confirmed_cause: Option<String>,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RepairUpdate {
    pub(crate) id: i64,
    pub(crate) to: &'static str,
    pub(crate) from: &'static [&'static str],
    pub(crate) branch: Option<String>,
    pub(crate) base_sha: Option<String>,
    pub(crate) commit_sha: Option<String>,
    pub(crate) patch: Option<Value>,
    pub(crate) gate_exit: Option<i32>,
    pub(crate) gate_output: Option<String>,
    pub(crate) gate_output_bytes: Option<i32>,
    pub(crate) pr_url: Option<String>,
    pub(crate) merge_sha: Option<String>,
    pub(crate) reason: Option<String>,
    pub(crate) interrupted: bool,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RepairFilter {
    pub(crate) problem_id: Option<i64>,
    pub(crate) before: Option<i64>,
    pub(crate) limit: i64,
}

impl crate::store::persistence::web_repo::WebDb<'_> {
    pub(crate) async fn claim_wright_repairs(
        self,
        worktrees: &str,
        machine: Option<Uuid>,
        limit: i64,
    ) -> Result<Vec<ClaimedRepair>, PersistError> {
        Ok(
            sqlx::query_as(CLAIM) // grader-allow: reads wright.findings, which the engineer-run migration (#73) creates on its own branch, so query_as! cannot check it here; Wright's store runs include_str! SQL at runtime
                .bind(worktrees)
                .bind(machine)
                .bind(limit)
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub(crate) async fn advance_wright_repair(
        self,
        update: &RepairUpdate,
    ) -> Result<bool, PersistError> {
        let from: Vec<&str> = update.from.to_vec();
        let moved: Option<i64> = sqlx::query_scalar(ADVANCE)
            .bind(update.id)
            .bind(update.to)
            .bind(update.branch.as_deref())
            .bind(update.base_sha.as_deref())
            .bind(update.commit_sha.as_deref())
            .bind(update.patch.as_ref())
            .bind(update.gate_exit)
            .bind(update.gate_output.as_deref())
            .bind(update.gate_output_bytes)
            .bind(update.pr_url.as_deref())
            .bind(update.merge_sha.as_deref())
            .bind(update.reason.as_deref())
            .bind(&from)
            .bind(update.interrupted)
            .fetch_optional(self.pool())
            .await?;
        Ok(moved.is_some())
    }

    pub(crate) async fn unclaim_wright_repair(self, id: i64) -> Result<bool, PersistError> {
        let gone: Option<i64> = sqlx::query_scalar(UNCLAIM)
            .bind(id)
            .fetch_optional(self.pool())
            .await?;
        Ok(gone.is_some())
    }

    pub(crate) async fn wright_repair_runs_failed(
        self,
        problem_id: i64,
    ) -> Result<i64, PersistError> {
        Ok(sqlx::query_scalar(FAILED_RUNS)
            .bind(problem_id)
            .fetch_one(self.pool())
            .await?)
    }

    pub(crate) async fn wright_repair_waiting(self) -> Result<bool, PersistError> {
        Ok(sqlx::query_scalar(WAITING).fetch_one(self.pool()).await?)
    }

    pub(crate) async fn landed_wright_repairs(self) -> Result<Vec<(i64, String)>, PersistError> {
        Ok(
            sqlx::query_as(LANDED) // grader-allow: Wright's store runs its include_str! SQL at runtime (build plan, temet-ai/mike#71); no .sqlx entry
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub(crate) async fn mark_wright_repairs_deployed(
        self,
        ids: &[i64],
    ) -> Result<Vec<i64>, PersistError> {
        Ok(sqlx::query_scalar(MARK_DEPLOYED)
            .bind(ids)
            .fetch_all(self.pool())
            .await?)
    }

    pub(crate) async fn mark_wright_repairs_healthy(self) -> Result<Vec<i64>, PersistError> {
        Ok(sqlx::query_scalar(MARK_HEALTHY)
            .fetch_all(self.pool())
            .await?)
    }

    pub(crate) async fn list_wright_repairs(
        self,
        filter: &RepairFilter,
    ) -> Result<Vec<RepairRow>, PersistError> {
        Ok(
            sqlx::query_as(LIST) // grader-allow: Wright's store runs its include_str! SQL at runtime (build plan, temet-ai/mike#71); no .sqlx entry
                .bind(filter.problem_id)
                .bind(filter.before)
                .bind(filter.limit)
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub(crate) async fn wright_repair(self, id: i64) -> Result<Option<RepairRow>, PersistError> {
        Ok(
            sqlx::query_as(ONE) // grader-allow: Wright's store runs its include_str! SQL at runtime (build plan, temet-ai/mike#71); no .sqlx entry
                .bind(id)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub(crate) async fn interrupted_wright_repairs(
        self,
        machine: Option<Uuid>,
    ) -> Result<Vec<RepairRow>, PersistError> {
        Ok(
            sqlx::query_as(INTERRUPTED) // grader-allow: Wright's store runs its include_str! SQL at runtime (build plan, temet-ai/mike#71); no .sqlx entry
                .bind(machine)
                .fetch_all(self.pool())
                .await?,
        )
    }
}
