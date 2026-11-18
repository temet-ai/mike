use chrono::{DateTime, NaiveDate, Utc};
use serde::Serialize;
use uuid::Uuid;

use crate::store::persistence::PersistError;

const ENSURE: &str = include_str!("sql/fix_batch_ensure.sql");
const CLOSE_DUE: &str = include_str!("sql/fix_batch_close_due.sql");
const CLOSE_NOW: &str = include_str!("sql/fix_batch_close_now.sql");
const ADVANCE: &str = include_str!("sql/fix_batch_advance.sql");
const LANDED: &str = include_str!("sql/fix_batch_landed.sql");
const FIXES: &str = include_str!("sql/fix_batch_fixes.sql");
const ONE: &str = include_str!("sql/fix_batch_one.sql");
const LIST: &str = include_str!("sql/fix_batch_list.sql");
const TO_LAND: &str = include_str!("sql/fix_batch_to_land.sql");
const OPEN: &str = include_str!("sql/fix_batch_open.sql");
const REMAP_REPAIRS: &str = include_str!("sql/fix_batch_remap_repairs.sql");
const REMAP_FOLLOWUPS: &str = include_str!("sql/fix_batch_remap_followups.sql");
const EDITING: &str = include_str!("sql/repairs_editing.sql");

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub(crate) struct FixBatchRow {
    pub(crate) id: i64,
    pub(crate) day: NaiveDate,
    pub(crate) branch: String,
    pub(crate) branches: Vec<String>,
    pub(crate) worktree: String,
    pub(crate) machine_uuid: Option<Uuid>,
    pub(crate) base_sha: Option<String>,
    pub(crate) state: String,
    pub(crate) rounds: i32,
    pub(crate) gate_exit: Option<i32>,
    pub(crate) gate_output: Option<String>,
    pub(crate) gate_output_bytes: Option<i32>,
    pub(crate) pr_url: Option<String>,
    pub(crate) merge_sha: Option<String>,
    pub(crate) reason: Option<String>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) closed_at: Option<DateTime<Utc>>,
    pub(crate) landed_at: Option<DateTime<Utc>>,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub(crate) struct ClosedBatch {
    pub(crate) id: i64,
    pub(crate) worktree: String,
    pub(crate) branch: String,
    pub(crate) kept: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub(crate) struct BatchFix {
    pub(crate) kind: String,
    pub(crate) id: i64,
    pub(crate) commit_sha: Option<String>,
    pub(crate) finding_id: Option<i64>,
    pub(crate) problem_id: Option<i64>,
    pub(crate) owning_part: Option<String>,
    pub(crate) issue_number: Option<i32>,
    pub(crate) label: String,
}

impl BatchFix {
    pub(crate) fn is_repair(&self) -> bool {
        self.kind == "repair"
    }
}

#[derive(Debug, Clone, Default)]
pub(crate) struct BatchUpdate {
    pub(crate) id: i64,
    pub(crate) to: &'static str,
    pub(crate) from: &'static [&'static str],
    pub(crate) branch: Option<String>,
    pub(crate) base_sha: Option<String>,
    pub(crate) gate_exit: Option<i32>,
    pub(crate) gate_output: Option<String>,
    pub(crate) gate_output_bytes: Option<i32>,
    pub(crate) pr_url: Option<String>,
    pub(crate) merge_sha: Option<String>,
    pub(crate) reason: Option<String>,
    pub(crate) rounds_added: i32,
}

impl crate::store::persistence::web_repo::WebDb<'_> {
    pub(crate) async fn ensure_wright_fix_batch(
        self,
        day: NaiveDate,
        branch: &str,
        worktree: &str,
        machine: Option<Uuid>,
    ) -> Result<FixBatchRow, PersistError> {
        Ok(
            sqlx::query_as(ENSURE) // grader-allow: Wright's store runs its include_str! SQL at runtime; no .sqlx entry
                .bind(day)
                .bind(branch)
                .bind(worktree)
                .bind(machine)
                .fetch_one(self.pool())
                .await?,
        )
    }

    pub(crate) async fn close_due_wright_fix_batches(
        self,
        today: NaiveDate,
    ) -> Result<Vec<ClosedBatch>, PersistError> {
        Ok(
            sqlx::query_as(CLOSE_DUE) // grader-allow: Wright's store runs its include_str! SQL at runtime; no .sqlx entry
                .bind(today)
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub(crate) async fn close_wright_fix_batch_now(
        self,
        id: i64,
    ) -> Result<Option<FixBatchRow>, PersistError> {
        Ok(
            sqlx::query_as(CLOSE_NOW) // grader-allow: Wright's store runs its include_str! SQL at runtime; no .sqlx entry
                .bind(id)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub(crate) async fn advance_wright_fix_batch(
        self,
        update: &BatchUpdate,
    ) -> Result<bool, PersistError> {
        let from: Vec<&str> = update.from.to_vec();
        let moved: Option<i64> = sqlx::query_scalar(ADVANCE)
            .bind(update.id)
            .bind(update.to)
            .bind(&from)
            .bind(update.branch.as_deref())
            .bind(update.base_sha.as_deref())
            .bind(update.gate_exit)
            .bind(update.gate_output.as_deref())
            .bind(update.gate_output_bytes)
            .bind(update.pr_url.as_deref())
            .bind(update.merge_sha.as_deref())
            .bind(update.reason.as_deref())
            .bind(update.rounds_added)
            .fetch_optional(self.pool())
            .await?;
        Ok(moved.is_some())
    }

    pub(crate) async fn land_wright_fix_batch(
        self,
        id: i64,
        merge_sha: &str,
        pr_url: Option<&str>,
    ) -> Result<Option<Vec<i64>>, PersistError> {
        let rows: Vec<(i64, Option<i64>)> = sqlx::query_as(LANDED) // grader-allow: Wright's store runs its include_str! SQL at runtime; no .sqlx entry
            .bind(id)
            .bind(merge_sha)
            .bind(pr_url)
            .fetch_all(self.pool())
            .await?;
        if rows.is_empty() {
            return Ok(None);
        }
        Ok(Some(
            rows.into_iter().filter_map(|(_, repair)| repair).collect(),
        ))
    }

    pub(crate) async fn wright_fix_batch_fixes(
        self,
        batch: i64,
    ) -> Result<Vec<BatchFix>, PersistError> {
        Ok(
            sqlx::query_as(FIXES) // grader-allow: Wright's store runs its include_str! SQL at runtime; no .sqlx entry
                .bind(batch)
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub(crate) async fn wright_fix_batch(
        self,
        id: i64,
    ) -> Result<Option<FixBatchRow>, PersistError> {
        Ok(
            sqlx::query_as(ONE) // grader-allow: Wright's store runs its include_str! SQL at runtime; no .sqlx entry
                .bind(id)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub(crate) async fn list_wright_fix_batches(
        self,
        before: Option<i64>,
        limit: i64,
    ) -> Result<Vec<FixBatchRow>, PersistError> {
        Ok(
            sqlx::query_as(LIST) // grader-allow: Wright's store runs its include_str! SQL at runtime; no .sqlx entry
                .bind(before)
                .bind(limit)
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub(crate) async fn wright_fix_batches_to_land(
        self,
        machine: Option<Uuid>,
    ) -> Result<Vec<FixBatchRow>, PersistError> {
        Ok(
            sqlx::query_as(TO_LAND) // grader-allow: Wright's store runs its include_str! SQL at runtime; no .sqlx entry
                .bind(machine)
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub(crate) async fn open_wright_fix_batch(
        self,
        day: Option<NaiveDate>,
    ) -> Result<Option<FixBatchRow>, PersistError> {
        Ok(
            sqlx::query_as(OPEN) // grader-allow: Wright's store runs its include_str! SQL at runtime; no .sqlx entry
                .bind(day)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub(crate) async fn remap_wright_batch_commits(
        self,
        batch: i64,
        repairs: &[(i64, String)],
        followups: &[(i64, String)],
    ) -> Result<(), PersistError> {
        let (ids, shas): (Vec<i64>, Vec<String>) = repairs.iter().cloned().unzip();
        sqlx::query(REMAP_REPAIRS) // grader-allow: Wright's store runs its include_str! SQL at runtime; no .sqlx entry
            .bind(batch)
            .bind(&ids)
            .bind(&shas)
            .execute(self.pool())
            .await?;
        let (ids, shas): (Vec<i64>, Vec<String>) = followups.iter().cloned().unzip();
        sqlx::query(REMAP_FOLLOWUPS) // grader-allow: Wright's store runs its include_str! SQL at runtime; no .sqlx entry
            .bind(batch)
            .bind(&ids)
            .bind(&shas)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub(crate) async fn wright_repair_editing(self) -> Result<bool, PersistError> {
        Ok(sqlx::query_scalar(EDITING).fetch_one(self.pool()).await?)
    }
}
