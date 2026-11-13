mod progress;

use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::store::persistence::PersistError;
use crate::store::persistence::operator_turns::judge_claim::ClaimScope;
use crate::store::persistence::operator_turns::projection::OUTCOME_PROJECTION_VERSION;

pub use progress::{RejudgeProgress, RejudgeWindow};

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub struct RejudgeRunRow {
    pub id: i64,
    pub role: String,
    pub bundle_id: i64,
    pub window_since: DateTime<Utc>,
    pub window_until: DateTime<Utc>,
    pub daily_call_cap: i32,
    pub status: String,
    pub estimated_calls: i32,
    pub estimated_usd: Option<f64>,
    pub created_by: String,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
}

impl RejudgeRunRow {
    pub fn window(&self) -> RejudgeWindow {
        RejudgeWindow {
            bundle_id: self.bundle_id,
            since: self.window_since,
            until: self.window_until,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewRejudgeRun<'a> {
    pub window: RejudgeWindow,
    pub daily_call_cap: i32,
    pub estimated_calls: i32,
    pub estimated_usd: Option<f64>,
    pub created_by: &'a str,
    pub role: &'a str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RejudgeFinish {
    Done,
    Cancelled,
}

impl RejudgeFinish {
    fn code(self) -> &'static str {
        match self {
            Self::Done => "done",
            Self::Cancelled => "cancelled",
        }
    }
}

fn is_unique_violation(e: &sqlx::Error) -> bool {
    e.as_database_error()
        .and_then(|d| d.code())
        .is_some_and(|code| code == "23505")
}

impl crate::store::persistence::eval_repo::EvalDb<'_> {
    pub async fn create_rejudge_run(self, run: &NewRejudgeRun<'_>) -> Result<i64, PersistError> {
        crate::metrics::time_db("create_rejudge_run", async move {
            let mut tx = self.pool.begin().await?;
            let inserted = sqlx::query_file_scalar!(
                "src/store/persistence/eval_rejudge/insert_run.sql",
                run.window.bundle_id,
                run.window.since,
                run.window.until,
                run.daily_call_cap,
                run.estimated_calls,
                run.estimated_usd,
                run.created_by,
                run.role,
            )
            .fetch_one(&mut *tx)
            .await;
            let id = match inserted {
                Ok(id) => id,
                Err(e) if is_unique_violation(&e) => {
                    return Err(PersistError::DataInvariant(
                        "a re-judge run is already queued or running; cancel it first".to_owned(),
                    ));
                }
                Err(e) => return Err(e.into()),
            };
            if run.role == "turn_judge" {
                sqlx::query_file!(
                    "src/store/persistence/eval_rejudge/clear_failures.sql",
                    run.window.since,
                    run.window.until,
                    run.window.bundle_id,
                )
                .execute(&mut *tx)
                .await?;
            }
            tx.commit().await?;
            Ok(id)
        })
        .await
    }

    pub async fn active_rejudge_run(self) -> Result<Option<RejudgeRunRow>, PersistError> {
        crate::metrics::time_db("active_rejudge_run", async move {
            Ok(sqlx::query_file_as!(
                RejudgeRunRow,
                "src/store/persistence/eval_rejudge/run_active.sql",
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn list_rejudge_runs(self) -> Result<Vec<RejudgeRunRow>, PersistError> {
        crate::metrics::time_db("list_rejudge_runs", async move {
            Ok(sqlx::query_file_as!(
                RejudgeRunRow,
                "src/store/persistence/eval_rejudge/run_list.sql",
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn get_rejudge_run(self, id: i64) -> Result<Option<RejudgeRunRow>, PersistError> {
        crate::metrics::time_db("get_rejudge_run", async move {
            Ok(sqlx::query_file_as!(
                RejudgeRunRow,
                "src/store/persistence/eval_rejudge/run_get.sql",
                id,
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn start_rejudge_run(self, id: i64) -> Result<bool, PersistError> {
        crate::metrics::time_db("start_rejudge_run", async move {
            let r = sqlx::query!(
                "UPDATE eval.rejudge_runs
    SET status = 'running', started_at = now()
  WHERE id = $1 AND status = 'queued'",
                id,
            )
            .execute(&self.pool)
            .await?;
            Ok(r.rows_affected() > 0)
        })
        .await
    }

    pub async fn finish_rejudge_run(
        self,
        id: i64,
        finish: RejudgeFinish,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("finish_rejudge_run", async move {
            let r = sqlx::query!(
                "UPDATE eval.rejudge_runs
    SET status = $2, finished_at = now()
  WHERE id = $1 AND status IN ('queued', 'running')",
                id,
                finish.code(),
            )
            .execute(&self.pool)
            .await?;
            Ok(r.rows_affected() > 0)
        })
        .await
    }

    pub async fn rejudge_ineligible_turn_ids(
        self,
        window: &RejudgeWindow,
        after_id: i64,
        limit: i64,
    ) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("rejudge_ineligible_turn_ids", async move {
            Ok(sqlx::query_file_scalar!(
                "src/store/persistence/eval_rejudge/ineligible_turns.sql",
                window.since,
                window.until,
                after_id,
                limit,
                OUTCOME_PROJECTION_VERSION,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn claim_rejudge_candidates(
        self,
        window: &RejudgeWindow,
        limit: i64,
    ) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("claim_rejudge_candidates", async move {
            let scope = ClaimScope::ClosedBetween {
                since: window.since,
                until: window.until,
            };
            self.claim_judge_turns(window.bundle_id, scope, limit).await
        })
        .await
    }
}
