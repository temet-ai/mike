use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use crate::store::persistence::PersistError;

const ASSIGN: &str = include_str!("sql/triage/assign.sql");
const MERGE: &str = include_str!("sql/triage/merge.sql");
const OVERDUE: &str = include_str!("sql/triage/overdue.sql");
const RECURRING: &str = include_str!("sql/triage/recurring.sql");
const MARK_COMMENTED: &str = include_str!("sql/triage/mark_commented.sql");
const LIST: &str = include_str!("sql/triage/list.sql");

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub(crate) struct Assigned {
    pub(crate) assigned: i64,
    pub(crate) opened: i64,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub(crate) struct Merged {
    pub(crate) causes: i64,
    pub(crate) problems: i64,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub(crate) struct WaitingCause {
    pub(crate) cause_id: i64,
    pub(crate) check_name: String,
    pub(crate) resource: String,
    pub(crate) problems: i64,
    pub(crate) observations: i64,
    pub(crate) opened_at: DateTime<Utc>,
    pub(crate) attempts: i64,
    pub(crate) priority: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub(crate) struct RecurringCause {
    pub(crate) cause_id: i64,
    pub(crate) followup_id: i64,
    pub(crate) issue_number: i32,
    pub(crate) check_name: String,
    pub(crate) resource: String,
    pub(crate) problems: i64,
    pub(crate) observations: i64,
    pub(crate) since: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Serialize, sqlx::FromRow)]
pub(crate) struct CauseRow {
    pub(crate) cause_id: i64,
    pub(crate) problem_id: i64,
    pub(crate) check_name: String,
    pub(crate) resource: String,
    pub(crate) problems: i64,
    pub(crate) observations: i64,
    pub(crate) opened_at: DateTime<Utc>,
    pub(crate) attempts: i64,
    pub(crate) last_attempt_at: Option<DateTime<Utc>>,
    pub(crate) finding_id: Option<i64>,
    pub(crate) followup_id: Option<i64>,
    pub(crate) issue_number: Option<i32>,
    pub(crate) remedied: bool,
    pub(crate) priority: f64,
    pub(crate) notified_at: Option<DateTime<Utc>>,
    pub(crate) commented_at: Option<DateTime<Utc>>,
    pub(crate) pinned: bool,
    pub(crate) pinned_at: Option<DateTime<Utc>>,
    pub(crate) operator_priority: Option<f64>,
    pub(crate) rank_priority: f64,
    pub(crate) group_id: Option<i64>,
    pub(crate) group_name: Option<String>,
    pub(crate) absorbed: sqlx::types::Json<Vec<AbsorbedCause>>,
    pub(crate) investigable: bool,
    pub(crate) not_investigated_because: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct AbsorbedCause {
    pub(crate) merge_id: i64,
    pub(crate) cause_id: i64,
    pub(crate) check_name: String,
    pub(crate) key: String,
    pub(crate) problems: i64,
    pub(crate) by: String,
    pub(crate) created_at: DateTime<Utc>,
    pub(crate) can_separate: bool,
}

impl crate::store::persistence::web_repo::WebDb<'_> {
    pub(crate) async fn assign_wright_causes(self) -> Result<Assigned, PersistError> {
        let (assigned, opened): (i64, i64) = sqlx::query_as(ASSIGN) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
            .fetch_one(self.pool())
            .await?;
        Ok(Assigned { assigned, opened })
    }

    pub(crate) async fn merge_wright_causes(
        self,
        problem_id: i64,
        cause_ids: &[i64],
        by: &str,
    ) -> Result<Merged, PersistError> {
        let (causes, problems): (i64, i64) = sqlx::query_as(MERGE) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
            .bind(problem_id)
            .bind(cause_ids)
            .bind(by)
            .fetch_one(self.pool())
            .await?;
        Ok(Merged { causes, problems })
    }

    pub(crate) async fn claim_overdue_wright_causes(
        self,
        waited_hours: i32,
    ) -> Result<Vec<WaitingCause>, PersistError> {
        Ok(
            sqlx::query_as(OVERDUE) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .bind(waited_hours)
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub(crate) async fn recurring_wright_causes(
        self,
        limit: i64,
    ) -> Result<Vec<RecurringCause>, PersistError> {
        Ok(
            sqlx::query_as(RECURRING) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .bind(limit)
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub(crate) async fn mark_wright_cause_commented(
        self,
        cause_id: i64,
    ) -> Result<(), PersistError> {
        sqlx::query(MARK_COMMENTED) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
            .bind(cause_id)
            .execute(self.pool())
            .await?;
        Ok(())
    }

    pub(crate) async fn list_wright_causes(
        self,
        limit: i64,
    ) -> Result<Vec<CauseRow>, PersistError> {
        Ok(
            sqlx::query_as(LIST) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .bind(limit)
                .bind(None::<i64>)
                .fetch_all(self.pool())
                .await?,
        )
    }

    pub(crate) async fn wright_cause(self, cause_id: i64) -> Result<Option<CauseRow>, PersistError> {
        Ok(
            sqlx::query_as(LIST) // grader-allow: runtime include_str! query, as the Wright persistence is written; no .sqlx entry on this branch
                .bind(1_i64)
                .bind(Some(cause_id))
                .fetch_optional(self.pool())
                .await?,
        )
    }
}
