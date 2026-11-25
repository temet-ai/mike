use chrono::{DateTime, Utc};
use serde::Serialize;
use uuid::Uuid;

use super::code_scan::ReadyFinding;
use crate::store::persistence::PersistError;

#[derive(Debug, Clone)]
pub struct GroupFinding {
    pub id: i64,
    pub line: i32,
    pub severity: String,
    pub message: String,
    pub matched: String,
    pub checkout: String,
    pub path: String,
    pub rule_id: String,
    pub refusals: i32,
    pub author: Option<String>,
    pub tool_name: Option<String>,
    pub named: bool,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperatorFinding {
    pub id: i64,
    pub session_id: Option<Uuid>,
    pub checkout: String,
    pub path: String,
    pub rule_id: String,
    pub severity: String,
    pub message: String,
    pub lines: Vec<i32>,
    pub answer: String,
    pub answer_reason: Option<String>,
    pub answer_ruling: Option<String>,
    pub since: DateTime<Utc>,
}

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn code_answer_group(
        self,
        finding: i64,
        session: Uuid,
    ) -> Result<Vec<GroupFinding>, PersistError> {
        crate::metrics::time_db("code_answer_group", async move {
            let rows = sqlx::query_file_as!(
                GroupFinding,
                "src/store/persistence/change_record/code_scan/answer_group.sql",
                finding,
                session,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn code_keeps_today(self, session: Uuid, rule: &str) -> Result<i64, PersistError> {
        crate::metrics::time_db("code_answer_keeps_today", async move {
            let keeps = sqlx::query_file_scalar!(
                "src/store/persistence/change_record/code_scan/answer_keeps_today.sql",
                session,
                rule,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(keeps)
        })
        .await
    }

    pub async fn record_code_answer(
        self,
        ids: &[i64],
        answer: &str,
        reason: &str,
        ruling: &str,
    ) -> Result<u64, PersistError> {
        crate::metrics::time_db("code_answer_record", async move {
            let done = sqlx::query_file!(
                "src/store/persistence/change_record/code_scan/answer_record.sql",
                ids,
                answer,
                reason,
                ruling,
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }

    pub async fn accept_waiting_code_answer(self, finding: i64) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("code_answer_accept", async move {
            let ids = sqlx::query_file_scalar!(
                "src/store/persistence/change_record/code_scan/accept_waiting.sql",
                finding,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(ids)
        })
        .await
    }

    pub async fn code_findings_to_remind(
        self,
        checkouts: &[String],
    ) -> Result<Vec<ReadyFinding>, PersistError> {
        crate::metrics::time_db("code_findings_remind", async move {
            let rows = sqlx::query_file_as!(
                ReadyFinding,
                "src/store/persistence/change_record/code_scan/remind.sql",
                checkouts,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn code_findings_for_operator(self) -> Result<Vec<OperatorFinding>, PersistError> {
        crate::metrics::time_db("code_findings_for_operator", async move {
            let rows = sqlx::query_file_as!(
                OperatorFinding,
                "src/store/persistence/change_record/code_scan/for_operator.sql",
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
