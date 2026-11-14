use chrono::{DateTime, Utc};
use serde_json::Value;

use crate::store::persistence::PersistError;

const PENDING_STEERS: &str = include_str!("sql/conversation/pending_steers.sql");
const STEER: &str = include_str!("sql/conversation/steer.sql");
const STEER_APPLIED: &str = include_str!("sql/conversation/steer_applied.sql");
const STEER_REFUSED: &str = include_str!("sql/conversation/steer_refused.sql");
const STEER_UNDONE: &str = include_str!("sql/conversation/steer_undone.sql");
const PROBLEM_BRIEF: &str = include_str!("sql/conversation/problem_brief.sql");
const OPERATOR_PROBLEM: &str = include_str!("sql/conversation/operator_problem.sql");
const CHECK_EXISTS: &str = include_str!("sql/conversation/check_exists.sql");
const OPEN_OPERATOR_PROBLEM: &str = include_str!("sql/conversation/open_operator_problem.sql");
const UNDO_OPERATOR_PROBLEM: &str = include_str!("sql/conversation/undo_operator_problem.sql");
const ACTIVE_SKIPS: &str = include_str!("sql/conversation/active_skips.sql");

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub(crate) struct SteerRow {
    pub(crate) id: i64,
    pub(crate) message_id: i64,
    pub(crate) change: Value,
    pub(crate) prior: Option<Value>,
    pub(crate) effect: Option<Value>,
    pub(crate) refusal: Option<String>,
    pub(crate) applied_at: Option<DateTime<Utc>>,
    pub(crate) undone_at: Option<DateTime<Utc>>,
    pub(crate) version: i32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct OperatorProblemUndo {
    pub(crate) problem_id: i64,
    pub(crate) episode_id: i64,
    pub(crate) run_id: i64,
    pub(crate) prior_healthy_at: Option<DateTime<Utc>>,
}

impl crate::store::persistence::web_repo::WebDb<'_> {
    pub(crate) async fn pending_wright_steers(
        self,
        limit: i64,
    ) -> Result<Vec<SteerRow>, PersistError> {
        Ok(
            sqlx::query_as(PENDING_STEERS) // grader-allow: runtime query over an include_str! file; no .sqlx entry for the wright conversation tables
                .bind(limit)
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub(crate) async fn wright_steer(self, id: i64) -> Result<Option<SteerRow>, PersistError> {
        Ok(
            sqlx::query_as(STEER) // grader-allow: runtime query over an include_str! file; no .sqlx entry for the wright conversation tables
                .bind(id)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub(crate) async fn mark_wright_steer_applied(
        self,
        id: i64,
        effect: &Value,
    ) -> Result<bool, PersistError> {
        let done = sqlx::query(STEER_APPLIED) // grader-allow: runtime query over an include_str! file; no .sqlx entry for the wright conversation tables
            .bind(id)
            .bind(effect)
            .execute(self.pool())
            .await?;
        Ok(done.rows_affected() == 1)
    }

    pub(crate) async fn refuse_wright_steer(
        self,
        id: i64,
        reason: &str,
    ) -> Result<bool, PersistError> {
        let done = sqlx::query(STEER_REFUSED) // grader-allow: runtime query over an include_str! file; no .sqlx entry for the wright conversation tables
            .bind(id)
            .bind(reason)
            .execute(self.pool())
            .await?;
        Ok(done.rows_affected() == 1)
    }

    pub(crate) async fn mark_wright_steer_undone(
        self,
        id: i64,
        version: i32,
    ) -> Result<Option<SteerRow>, PersistError> {
        Ok(
            sqlx::query_as(STEER_UNDONE) // grader-allow: runtime query over an include_str! file; no .sqlx entry for the wright conversation tables
                .bind(id)
                .bind(version)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub(crate) async fn wright_problem_brief(self, id: i64) -> Result<Option<Value>, PersistError> {
        Ok(sqlx::query_scalar(PROBLEM_BRIEF)
            .bind(id)
            .fetch_optional(self.pool())
            .await?)
    }

    pub(crate) async fn wright_operator_problem(
        self,
        check: &str,
        resource: &str,
    ) -> Result<Option<Value>, PersistError> {
        Ok(sqlx::query_scalar(OPERATOR_PROBLEM)
            .bind(check)
            .bind(resource)
            .fetch_optional(self.pool())
            .await?)
    }

    pub(crate) async fn wright_check_exists(self, name: &str) -> Result<bool, PersistError> {
        Ok(sqlx::query_scalar(CHECK_EXISTS)
            .bind(name)
            .fetch_one(self.pool())
            .await?)
    }

    pub(crate) async fn open_wright_operator_problem(
        self,
        check: &str,
        resource: &str,
        steer_id: i64,
    ) -> Result<Option<Value>, PersistError> {
        Ok(sqlx::query_scalar(OPEN_OPERATOR_PROBLEM)
            .bind(check)
            .bind(resource)
            .bind(steer_id)
            .fetch_optional(self.pool())
            .await?)
    }

    pub(crate) async fn undo_wright_operator_problem(
        self,
        undo: &OperatorProblemUndo,
    ) -> Result<bool, PersistError> {
        let matched: i64 = sqlx::query_scalar(UNDO_OPERATOR_PROBLEM)
            .bind(undo.problem_id)
            .bind(undo.episode_id)
            .bind(undo.run_id)
            .bind(undo.prior_healthy_at)
            .fetch_one(self.pool())
            .await?;
        Ok(matched == 1)
    }

    pub(crate) async fn active_wright_skips(
        self,
    ) -> Result<Vec<(i64, DateTime<Utc>)>, PersistError> {
        Ok(
            sqlx::query_as(ACTIVE_SKIPS) // grader-allow: runtime query over an include_str! file; no .sqlx entry for the wright conversation tables
                .fetch_all(self.pool())
                .await?,
        )
    }
}
