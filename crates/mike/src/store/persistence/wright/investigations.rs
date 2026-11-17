use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use uuid::Uuid;

use super::EpisodeRow;
use crate::store::persistence::PersistError;

const LIVE_RUNS: &str = include_str!("sql/engineer/live_runs.sql");
const RECONCILE: &str = include_str!("sql/engineer/reconcile.sql");
const INSERT_RUN: &str = include_str!("sql/engineer/insert_run.sql");
const DAILY_STARTS: &str = include_str!("sql/engineer/daily_starts.sql");
const MARK_RUNNING: &str = include_str!("sql/engineer/mark_running.sql");
const USAGE: &str = include_str!("sql/engineer/usage.sql");
const END_RUN: &str = include_str!("sql/engineer/end_run.sql");
const LIST_RUNS: &str = include_str!("sql/engineer/list_runs.sql");
const ONE_RUN: &str = include_str!("sql/engineer/one_run.sql");
const INSERT_FINDING: &str = include_str!("sql/engineer/insert_finding.sql");
const LIST_FINDINGS: &str = include_str!("sql/engineer/list_findings.sql");
const ONE_FINDING: &str = include_str!("sql/engineer/one_finding.sql");
const LIMITS: &str = include_str!("sql/engineer/limits.sql");
const PATCH_LIMITS: &str = include_str!("sql/engineer/patch_limits.sql");
const PICK_PROBLEM: &str = include_str!("sql/engineer/pick_problem.sql");
const API_BACKOFF: &str = include_str!("sql/engineer/api_backoff.sql");
const ONE_EPISODE: &str = include_str!("sql/engineer/one_episode.sql");
const ONE_LIVE_INDEX: &str = "uq_wright_investigations_one_live";

type StartedRow = (i64, Option<i64>, DateTime<Utc>);

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub(crate) struct InvestigationRow {
    pub(crate) id: i64,
    pub(crate) mode: String,
    pub(crate) problem_id: Option<i64>,
    pub(crate) episode_id: Option<i64>,
    pub(crate) session_uuid: Uuid,
    pub(crate) state: String,
    pub(crate) host: String,
    pub(crate) machine_uuid: Option<Uuid>,
    pub(crate) pid: Option<i32>,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) started_at: Option<DateTime<Utc>>,
    pub(crate) deadline_at: DateTime<Utc>,
    pub(crate) ended_at: Option<DateTime<Utc>>,
    pub(crate) tool_calls: i32,
    pub(crate) tokens: i64,
    pub(crate) exit_code: Option<i32>,
    pub(crate) end_reason: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub(crate) struct FindingRow {
    pub(crate) id: i64,
    pub(crate) investigation_id: i64,
    pub(crate) problem_id: Option<i64>,
    pub(crate) episode_id: Option<i64>,
    pub(crate) symptom: String,
    pub(crate) suspected_cause: String,
    pub(crate) confirmed_cause: Option<String>,
    pub(crate) cause_established: bool,
    pub(crate) owning_part: Option<String>,
    pub(crate) kind: String,
    pub(crate) remedy: String,
    pub(crate) evidence: Value,
    pub(crate) created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub(crate) struct EngineerLimits {
    pub(crate) daily_starts: i32,
    pub(crate) wall_seconds: i32,
    pub(crate) tool_calls: i32,
    pub(crate) tokens: i64,
    pub(crate) updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct LimitsPatch {
    pub(crate) daily_starts: Option<i32>,
    pub(crate) wall_seconds: Option<i32>,
    pub(crate) tool_calls: Option<i32>,
    pub(crate) tokens: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub(crate) struct LiveRun {
    pub(crate) id: i64,
    pub(crate) state: String,
    pub(crate) host: String,
    pub(crate) machine_uuid: Option<Uuid>,
    pub(crate) pid: Option<i32>,
    pub(crate) deadline_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub(crate) struct NewInvestigation<'a> {
    pub(crate) mode: &'a str,
    pub(crate) problem_id: Option<i64>,
    pub(crate) session_uuid: Uuid,
    pub(crate) host: &'a str,
    pub(crate) machine_uuid: Option<Uuid>,
    pub(crate) owner_pid: i32,
    pub(crate) followup_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Inserted {
    Started {
        id: i64,
        episode_id: Option<i64>,
        deadline_at: DateTime<Utc>,
    },
    Busy,
    DailyLimit {
        started: i64,
        limit: i32,
    },
}

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub(crate) struct Usage {
    pub(crate) tool_calls: i32,
    pub(crate) tokens: i64,
    pub(crate) deadline_at: DateTime<Utc>,
    pub(crate) state: String,
    pub(crate) max_tool_calls: i32,
    pub(crate) max_tokens: i64,
}

#[derive(Debug, Clone, PartialEq)]
pub(crate) struct NewFinding {
    pub(crate) symptom: String,
    pub(crate) suspected_cause: String,
    pub(crate) confirmed_cause: Option<String>,
    pub(crate) cause_established: bool,
    pub(crate) owning_part: Option<String>,
    pub(crate) kind: String,
    pub(crate) remedy: String,
    pub(crate) evidence: Value,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, sqlx::FromRow)]
pub(crate) struct Pick {
    pub(crate) problem_id: i64,
    pub(crate) episode_id: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct PageQuery {
    pub(crate) problem_id: Option<i64>,
    pub(crate) before: Option<i64>,
    pub(crate) limit: i64,
}

fn one_live_violation(error: &sqlx::Error) -> bool {
    matches!(error, sqlx::Error::Database(db) if db.constraint() == Some(ONE_LIVE_INDEX))
}

impl crate::store::persistence::web_repo::WebDb<'_> {
    pub(crate) async fn live_wright_runs(self) -> Result<Vec<LiveRun>, PersistError> {
        Ok(
            sqlx::query_as(LIVE_RUNS) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub(crate) async fn reconcile_wright_runs(
        self,
        ids: &[i64],
        reasons: &[String],
    ) -> Result<Vec<i64>, PersistError> {
        Ok(sqlx::query_scalar(RECONCILE)
            .bind(ids)
            .bind(reasons)
            .fetch_all(self.pool())
            .await?)
    }

    pub(crate) async fn insert_wright_run(
        self,
        run: &NewInvestigation<'_>,
    ) -> Result<Inserted, PersistError> {
        let row: Result<Option<StartedRow>, sqlx::Error> = sqlx::query_as(INSERT_RUN) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
            .bind(run.mode)
            .bind(run.problem_id)
            .bind(run.session_uuid)
            .bind(run.host)
            .bind(run.machine_uuid)
            .bind(run.owner_pid)
            .bind(run.followup_id)
            .fetch_optional(self.pool())
            .await;
        match row {
            Ok(Some((id, episode_id, deadline_at))) => Ok(Inserted::Started {
                id,
                episode_id,
                deadline_at,
            }),
            Ok(None) => {
                let (started, limit): (i64, i32) = sqlx::query_as(DAILY_STARTS) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                    .fetch_one(self.pool())
                    .await?;
                Ok(Inserted::DailyLimit { started, limit })
            }
            Err(error) if one_live_violation(&error) => Ok(Inserted::Busy),
            Err(error) => Err(error.into()),
        }
    }

    pub(crate) async fn mark_wright_run_running(
        self,
        id: i64,
        pid: i32,
    ) -> Result<bool, PersistError> {
        let marked: Option<i64> = sqlx::query_scalar(MARK_RUNNING)
            .bind(id)
            .bind(pid)
            .fetch_optional(self.pool())
            .await?;
        Ok(marked.is_some())
    }

    pub(crate) async fn record_wright_usage(self, id: i64) -> Result<Option<Usage>, PersistError> {
        Ok(
            sqlx::query_as(USAGE) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .bind(id)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub(crate) async fn end_wright_run(
        self,
        id: i64,
        state: &str,
        exit_code: Option<i32>,
        reason: Option<&str>,
    ) -> Result<bool, PersistError> {
        let ended: Option<i64> = sqlx::query_scalar(END_RUN)
            .bind(id)
            .bind(state)
            .bind(exit_code)
            .bind(reason)
            .fetch_optional(self.pool())
            .await?;
        Ok(ended.is_some())
    }

    pub(crate) async fn finish_wright_run_with_finding(
        self,
        id: i64,
        exit_code: Option<i32>,
        finding: &NewFinding,
    ) -> Result<Option<i64>, PersistError> {
        Ok(sqlx::query_scalar(INSERT_FINDING)
            .bind(id)
            .bind(&finding.symptom)
            .bind(&finding.suspected_cause)
            .bind(finding.confirmed_cause.as_deref())
            .bind(finding.cause_established)
            .bind(finding.owning_part.as_deref())
            .bind(&finding.kind)
            .bind(&finding.remedy)
            .bind(&finding.evidence)
            .bind(exit_code)
            .fetch_optional(self.pool())
            .await?)
    }

    pub(crate) async fn list_wright_runs(
        self,
        page: PageQuery,
    ) -> Result<Vec<InvestigationRow>, PersistError> {
        Ok(
            sqlx::query_as(LIST_RUNS) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .bind(page.problem_id)
                .bind(page.before)
                .bind(page.limit)
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub(crate) async fn wright_run(
        self,
        id: i64,
    ) -> Result<Option<InvestigationRow>, PersistError> {
        Ok(
            sqlx::query_as(ONE_RUN) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .bind(id)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub(crate) async fn list_wright_findings(
        self,
        page: PageQuery,
    ) -> Result<Vec<FindingRow>, PersistError> {
        Ok(
            sqlx::query_as(LIST_FINDINGS) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .bind(page.problem_id)
                .bind(page.before)
                .bind(page.limit)
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub(crate) async fn wright_finding(self, id: i64) -> Result<Option<FindingRow>, PersistError> {
        Ok(
            sqlx::query_as(ONE_FINDING) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .bind(id)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub(crate) async fn wright_engineer_limits(self) -> Result<EngineerLimits, PersistError> {
        Ok(
            sqlx::query_as(LIMITS) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .fetch_one(self.pool())
                .await?,
        )
    }

    pub(crate) async fn patch_wright_engineer_limits(
        self,
        patch: LimitsPatch,
    ) -> Result<EngineerLimits, PersistError> {
        Ok(
            sqlx::query_as(PATCH_LIMITS) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .bind(patch.daily_starts)
                .bind(patch.wall_seconds)
                .bind(patch.tool_calls)
                .bind(patch.tokens)
                .fetch_one(self.pool())
                .await?,
        )
    }

    pub(crate) async fn wright_engineer_backing_off(self) -> Result<bool, PersistError> {
        Ok(
            sqlx::query_scalar(API_BACKOFF) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .fetch_one(self.pool())
                .await?,
        )
    }

    pub(crate) async fn pick_wright_problem(
        self,
        max_attempts: i64,
    ) -> Result<Option<Pick>, PersistError> {
        Ok(
            sqlx::query_as(PICK_PROBLEM) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .bind(max_attempts)
                .fetch_optional(self.pool())
                .await?,
        )
    }

    pub(crate) async fn wright_episode(self, id: i64) -> Result<Option<EpisodeRow>, PersistError> {
        Ok(
            sqlx::query_as(ONE_EPISODE) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .bind(id)
                .fetch_optional(self.pool())
                .await?,
        )
    }
}
