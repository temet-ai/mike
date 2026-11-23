use crate::store::persistence::PersistError;
use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;

#[derive(Debug, Clone, Serialize)]
pub struct NewDecision {
    pub run_id: i64,
    pub project_id: Option<i64>,
    pub subject_id: Option<i64>,
    pub statement_id: Option<i64>,
    pub item_id: Option<i64>,
    pub carrier_reference: String,
    pub verdict: String,
    pub counterpart_statement_id: Option<i64>,
    pub counterpart_reference: Option<String>,
    pub text_before: String,
    pub text_after: Option<String>,
    pub reason: String,
    pub evidence: Value,
    pub model: String,
    pub application: String,
    pub applied_at: Option<DateTime<Utc>>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DecisionApplication {
    Applied,
    Proposed,
    Failed,
    Undone,
    None,
}

impl DecisionApplication {
    pub fn code(self) -> &'static str {
        match self {
            Self::Applied => "applied",
            Self::Proposed => "proposed",
            Self::Failed => "failed",
            Self::Undone => "undone",
            Self::None => "none",
        }
    }
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn write_consolidation_decisions(
        self,
        decisions: &[NewDecision],
    ) -> Result<Vec<i64>, PersistError> {
        if decisions.is_empty() {
            return Ok(Vec::new());
        }
        let payload = serde_json::to_value(decisions)?;
        let ids = sqlx::query_file_scalar!(
            "src/store/persistence/consolidation/insert_decisions.sql",
            payload
        )
        .fetch_all(self.pool())
        .await?;
        Ok(ids)
    }

    pub async fn set_decision_application(
        self,
        decision_id: i64,
        application: DecisionApplication,
        error: Option<&str>,
    ) -> Result<bool, PersistError> {
        Ok(sqlx::query!(
            "UPDATE memory.knowledge_consolidation_decisions
                SET application = $2, error = $3,
                    applied_at = CASE WHEN $2 = 'applied' THEN now() ELSE applied_at END
              WHERE id = $1",
            decision_id,
            application.code(),
            error
        )
        .execute(self.pool())
        .await?
        .rows_affected()
            > 0)
    }

    pub async fn append_decision_evidence(
        self,
        decision_id: i64,
        items: &Value,
    ) -> Result<bool, PersistError> {
        Ok(sqlx::query!(
            "UPDATE memory.knowledge_consolidation_decisions
                SET evidence = evidence || $2::jsonb
              WHERE id = $1",
            decision_id,
            items
        )
        .execute(self.pool())
        .await?
        .rows_affected()
            > 0)
    }
}
