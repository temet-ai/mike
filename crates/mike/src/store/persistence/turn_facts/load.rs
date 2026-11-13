use chrono::{DateTime, Utc};

use super::TurnFactsRow;
use crate::store::persistence::PersistError;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TurnRecord {
    pub turn_id: i64,
    pub session_id: i64,
    pub start_request_id: i64,
    pub end_request_id: i64,
    pub created_at: DateTime<Utc>,
    pub closed_at: Option<DateTime<Utc>>,
    pub opened_by: Option<String>,
    pub outcome_evidence_request_id: Option<i64>,
    pub next_start: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct SessionRecord {
    pub session_id: i64,
    pub last_seen: DateTime<Utc>,
    pub project_path: Option<String>,
    pub role: Option<String>,
    pub is_template_session: bool,
    pub first_prompt: Option<String>,
    pub primary_without_agent_id: i64,
    pub agent_id_requests: i64,
    pub workflow_agent_requests: i64,
    pub launches: serde_json::Value,
}

impl crate::store::persistence::dossier_repo::DossierDb<'_> {
    pub async fn load_turn_records(
        self,
        session_ids: &[i64],
    ) -> Result<Vec<TurnRecord>, PersistError> {
        crate::metrics::time_db("load_turn_records", async move {
            Ok(sqlx::query_as!(
                TurnRecord,
                r#"SELECT t.id AS turn_id, t.session_id, t.start_request_id,
                          t.end_request_id, t.created_at, t.closed_at, t.opened_by,
                          t.outcome_evidence_request_id,
                          lead(t.start_request_id) OVER (
                              PARTITION BY t.session_id ORDER BY t.start_request_id
                          ) AS next_start
                     FROM memory.operator_turns t
                    WHERE t.session_id = ANY($1)
                    ORDER BY t.session_id, t.start_request_id"#,
                session_ids,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn load_session_records(
        self,
        session_ids: &[i64],
    ) -> Result<Vec<SessionRecord>, PersistError> {
        crate::metrics::time_db("load_session_records", async move {
            Ok(sqlx::query_file_as!(
                SessionRecord,
                "src/store/persistence/turn_facts/session_records.sql",
                session_ids,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn load_existing_turn_facts(
        self,
        session_ids: &[i64],
    ) -> Result<Vec<TurnFactsRow>, PersistError> {
        crate::metrics::time_db("load_existing_turn_facts", async move {
            Ok(sqlx::query_file_as!(
                TurnFactsRow,
                "src/store/persistence/turn_facts/existing_facts.sql",
                session_ids,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn load_max_request_ids(
        self,
        turn_ids: &[i64],
    ) -> Result<Vec<(i64, i64)>, PersistError> {
        crate::metrics::time_db("load_max_request_ids", async move {
            let rows = sqlx::query!(
                r#"SELECT r.operator_turn_id AS "turn_id!", max(r.id) AS "max_request_id!"
                     FROM requests.requests r
                    WHERE r.operator_turn_id = ANY($1)
                    GROUP BY r.operator_turn_id"#,
                turn_ids,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| (r.turn_id, r.max_request_id))
                .collect())
        })
        .await
    }
}
