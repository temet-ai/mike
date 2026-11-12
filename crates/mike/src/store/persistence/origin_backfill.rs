use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

use super::{PersistError, PersistHandle};
use crate::wire::genuine::markers::{WORKFLOW_HARNESS_USER_PREFIX, WORKFLOW_SUBAGENT_MARKERS};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct RelanePlanRow {
    pub id: i64,
    pub created_at: DateTime<Utc>,
    pub old_origin: String,
    pub new_origin: String,
    pub old_author: Option<String>,
    pub new_author: String,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct AgentOpenedTurn {
    pub turn_id: i64,
    pub start_request_id: i64,
    pub started_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, sqlx::FromRow)]
pub struct ClosedByAgentTurn {
    pub turn_id: i64,
    pub start_request_id: i64,
    pub closing_request_id: i64,
    pub outcome: Option<String>,
    pub judge_model: Option<String>,
    pub judge_rubric_version: Option<String>,
}

fn workflow_system_patterns() -> Vec<String> {
    WORKFLOW_SUBAGENT_MARKERS
        .iter()
        .map(|marker| format!("%{marker}%"))
        .collect()
}

fn workflow_prefix_regex() -> String {
    format!("^\\s*{}", regex::escape(WORKFLOW_HARNESS_USER_PREFIX))
}

impl PersistHandle {
    pub async fn origin_backfill_candidates(
        &self,
        window: (DateTime<Utc>, DateTime<Utc>),
        after: (DateTime<Utc>, i64),
        limit: i64,
    ) -> Result<Vec<(i64, DateTime<Utc>)>, PersistError> {
        let rows = sqlx::query!(
            "SELECT r.id, r.created_at
   FROM requests.requests r
   JOIN requests.turn_origins o ON o.id = r.turn_origin_id
  WHERE r.created_at >= $1 AND r.created_at < $2
    AND r.claude_code_agent_id IS NOT NULL
    AND o.code IN ('primary', 'primary_first')
    AND (r.created_at, r.id) > ($3, $4)
  ORDER BY r.created_at, r.id
  LIMIT $5",
            window.0,
            window.1,
            after.0,
            after.1,
            limit,
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.id, row.created_at))
            .collect())
    }

    pub async fn origin_backfill_plan(
        &self,
        request_ids: &[i64],
    ) -> Result<Vec<RelanePlanRow>, PersistError> {
        let patterns = workflow_system_patterns();
        let prefix = workflow_prefix_regex();
        let rows = sqlx::query_file_as!(
            RelanePlanRow,
            "src/store/persistence/origin_backfill/plan.sql",
            request_ids,
            &patterns,
            prefix,
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    pub async fn origin_backfill_relane(
        &self,
        plan: &[RelanePlanRow],
        window: (DateTime<Utc>, DateTime<Utc>),
    ) -> Result<u64, PersistError> {
        let payload: Vec<serde_json::Value> = plan
            .iter()
            .map(|row| {
                serde_json::json!({
                    "id": row.id,
                    "new_origin": row.new_origin,
                    "new_author": row.new_author,
                })
            })
            .collect();
        let done = sqlx::query!(
            "UPDATE requests.requests r
    SET turn_origin_id = new_origin.id,
        author_kind_id = new_author.id
   FROM jsonb_to_recordset($1::jsonb) AS x(id bigint, new_origin text, new_author text)
   JOIN requests.turn_origins new_origin ON new_origin.code = x.new_origin
   JOIN requests.author_kinds new_author ON new_author.code = x.new_author
  WHERE r.id = x.id
    AND r.created_at >= $2 AND r.created_at < $3
    AND (r.turn_origin_id, r.author_kind_id) IS DISTINCT FROM (new_origin.id, new_author.id)",
            serde_json::Value::Array(payload),
            window.0,
            window.1,
        )
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected())
    }

    pub async fn origin_backfill_unmarked_turns(
        &self,
        request_ids: &[i64],
    ) -> Result<Vec<AgentOpenedTurn>, PersistError> {
        let rows = sqlx::query_as!(
            AgentOpenedTurn,
            "SELECT t.id AS turn_id, t.start_request_id, r.created_at AS started_at
   FROM memory.operator_turns t
   JOIN requests.requests r ON r.id = t.start_request_id
  WHERE t.start_request_id = ANY($1)
    AND t.opened_by IS DISTINCT FROM 'agent'
  ORDER BY t.id",
            request_ids,
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }

    pub async fn origin_backfill_mark_agent_opened(
        &self,
        turn_ids: &[i64],
    ) -> Result<u64, PersistError> {
        let done = sqlx::query!(
            "UPDATE memory.operator_turns
    SET opened_by = 'agent'
  WHERE id = ANY($1) AND opened_by IS DISTINCT FROM 'agent'",
            turn_ids,
        )
        .execute(self.pool())
        .await?;
        Ok(done.rows_affected())
    }

    pub async fn origin_backfill_closed_by_agent(
        &self,
        window: (DateTime<Utc>, DateTime<Utc>),
    ) -> Result<Vec<ClosedByAgentTurn>, PersistError> {
        let rows = sqlx::query_as!(
            ClosedByAgentTurn,
            r#"SELECT t.id AS turn_id, t.start_request_id,
        t.outcome_evidence_request_id AS "closing_request_id!",
        tot.code AS outcome, t.judge_model, t.judge_rubric_version
   FROM requests.requests closer
   JOIN memory.operator_turns t ON t.outcome_evidence_request_id = closer.id
   JOIN requests.requests s ON s.id = t.start_request_id
   LEFT JOIN memory.turn_outcomes tot ON tot.id = t.outcome_id
  WHERE closer.created_at >= $1 AND closer.created_at < $2
    AND closer.claude_code_agent_id IS NOT NULL
    AND s.claude_code_agent_id IS NULL
  ORDER BY t.id"#,
            window.0,
            window.1,
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows)
    }
}
