use chrono::{DateTime, Utc};

use super::acceptance_criteria::{AcceptanceCriteriaMap, AcceptanceCriterionRow};
use super::memory_ask_ledger::AskLedgerEntryRow;
use super::session_action_list::{SessionActionCounts, SessionActionRow};
use super::turn_tool_call_reads::TurnToolCallMetaRow;
use super::{PersistError, PersistHandle};

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReplayTrigger {
    pub session_db_id: i64,
    pub session_uuid: uuid::Uuid,
    pub trigger_at: DateTime<Utc>,
    pub human_prompt: String,
    pub last_user_message: String,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct ReplayToolUse {
    pub tool_name: String,
    pub input: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct AsOfToolCallRow {
    pub tool_id: String,
    pub tool_name: String,
    pub input_bytes: Option<i32>,
    pub ord: i32,
    pub request_id: i64,
    pub has_result: bool,
    pub result_is_error: Option<bool>,
    pub result_content_bytes: Option<i64>,
    pub operation: Option<String>,
}

#[derive(sqlx::FromRow)]
struct CriteriaMapHead {
    id: i64,
    version: i32,
    ask_entry_count: i32,
    model: Option<String>,
    output_tokens: Option<i32>,
    created_at: DateTime<Utc>,
}

impl From<AsOfToolCallRow> for TurnToolCallMetaRow {
    fn from(row: AsOfToolCallRow) -> Self {
        Self {
            tool_id: row.tool_id,
            tool_name: row.tool_name,
            input_bytes: row.input_bytes,
            ord: row.ord,
            request_id: row.request_id,
            has_result: row.has_result,
            result_is_error: row.result_is_error,
            result_content_bytes: row.result_content_bytes,
            operation: row.operation,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct PriorFireRow {
    pub intervention_id: i64,
    pub operator_turn_id: Option<i64>,
    pub payload_text: Option<String>,
    pub missing_evidence: Option<String>,
    pub off_thread_topic: Option<String>,
    pub created_at: DateTime<Utc>,
}

impl PersistHandle {
    pub async fn fetch_replay_trigger(
        &self,
        trigger_request_id: i64,
    ) -> Result<ReplayTrigger, PersistError> {
        crate::metrics::time_db("fetch_replay_trigger", async {
            let row = sqlx::query_as!(
                ReplayTrigger,
                r#"SELECT r.session_id AS session_db_id,
                        s.session_uuid,
                        COALESCE(r.completed_at, r.created_at) AS "trigger_at!",
                        r.human_prompt,
                        r.last_user_message
                   FROM requests.requests r
                   JOIN sessions.sessions s ON s.id = r.session_id
                  WHERE r.id = $1"#,
                trigger_request_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            row.ok_or_else(|| {
                PersistError::DataInvariant(format!(
                    "verifier replay: trigger request {trigger_request_id} does not exist"
                ))
            })
        })
        .await
    }

    pub async fn fetch_replay_eval_text_blocks(
        &self,
        trigger_request_id: i64,
    ) -> Result<Vec<String>, PersistError> {
        crate::metrics::time_db("fetch_replay_eval_text_blocks", async {
            let rows = sqlx::query_scalar!(
                "SELECT b.content
                   FROM responses.stream_blocks b
                   JOIN responses.stream_block_types t ON t.id = b.block_type_id
                  WHERE b.request_id = $1
                    AND t.code = 'text'
                  ORDER BY b.seq_ord ASC",
                trigger_request_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn fetch_replay_trigger_tool_uses(
        &self,
        trigger_request_id: i64,
    ) -> Result<Vec<ReplayToolUse>, PersistError> {
        crate::metrics::time_db("fetch_replay_trigger_tool_uses", async {
            let rows = sqlx::query_as!(
                ReplayToolUse,
                "SELECT tu.tool_name, tu.input
                   FROM tools.uses tu
                  WHERE tu.request_id = $1
                  ORDER BY tu.ord ASC",
                trigger_request_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn fetch_ask_ledger_as_of(
        &self,
        session_db_id: i64,
        as_of: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<AskLedgerEntryRow>, PersistError> {
        crate::metrics::time_db("fetch_ask_ledger_as_of", async {
            let rows = sqlx::query_as!(
                AskLedgerEntryRow,
                r#"SELECT sub.id AS "id!", sub.session_id AS "session_id!", sub.operator_turn_id,
                        sub.kind AS "kind!", sub.content AS "content!", sub.created_at AS "created_at!"
                   FROM (
                        SELECT e.id, e.session_id, e.operator_turn_id, e.kind, e.content, e.created_at
                          FROM memory.ask_ledger_entries e
                         WHERE e.session_id = $1
                           AND e.created_at <= $2
                         ORDER BY e.created_at DESC
                         LIMIT $3) sub
                  ORDER BY sub.created_at ASC"#,
                session_db_id,
                as_of,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn fetch_turn_tool_calls_as_of(
        &self,
        turn_id: i64,
        session_db_id: i64,
        as_of: DateTime<Utc>,
    ) -> Result<Vec<AsOfToolCallRow>, PersistError> {
        crate::metrics::time_db("fetch_turn_tool_calls_as_of", async {
            let rows = sqlx::query_file_as!(
                AsOfToolCallRow,
                "src/store/persistence/verifier_replay/turn_tool_calls.sql",
                turn_id,
                session_db_id,
                as_of,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn fetch_session_action_list_as_of(
        &self,
        session_db_id: i64,
        as_of: DateTime<Utc>,
    ) -> Result<Vec<SessionActionRow>, PersistError> {
        crate::metrics::time_db("fetch_session_action_list_as_of", async {
            let rows = sqlx::query_file_as!(
                SessionActionRow,
                "src/store/persistence/verifier_replay/action_list.sql",
                session_db_id,
                as_of,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn fetch_session_action_list_recent_as_of(
        &self,
        session_db_id: i64,
        as_of: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<SessionActionRow>, PersistError> {
        crate::metrics::time_db("fetch_session_action_list_recent_as_of", async {
            let rows = sqlx::query_file_as!(
                SessionActionRow,
                "src/store/persistence/verifier_replay/action_list_recent.sql",
                session_db_id,
                as_of,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn fetch_session_action_counts_as_of(
        &self,
        session_db_id: i64,
        as_of: DateTime<Utc>,
    ) -> Result<SessionActionCounts, PersistError> {
        crate::metrics::time_db("fetch_session_action_counts_as_of", async {
            let counts = sqlx::query_file_as!(
                SessionActionCounts,
                "src/store/persistence/verifier_replay/action_counts.sql",
                session_db_id,
                as_of,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(counts)
        })
        .await
    }

    pub async fn fetch_prior_verifier_fires_as_of(
        &self,
        session_db_id: i64,
        steerer_id: i64,
        as_of: DateTime<Utc>,
        limit: i64,
        evaluated_trigger_request_id: i64,
    ) -> Result<Vec<PriorFireRow>, PersistError> {
        crate::metrics::time_db("fetch_prior_verifier_fires_as_of", async {
            let rows = sqlx::query_as!(
                PriorFireRow,
                r#"SELECT i.id AS "intervention_id!", i.operator_turn_id, i.payload_text,
                        i.missing_evidence, i.off_thread_topic, i.created_at
                   FROM steering.interventions i
                  WHERE i.session_id = $1
                    AND i.steerer_id = $2
                    AND i.created_at < $3
                    AND i.trigger_request_id IS DISTINCT FROM $5
                  ORDER BY i.created_at DESC, i.id DESC
                  LIMIT $4"#,
                session_db_id,
                steerer_id,
                as_of,
                limit,
                evaluated_trigger_request_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn fetch_open_turn_prompt_as_of(
        &self,
        session_db_id: i64,
        as_of: DateTime<Utc>,
    ) -> Result<Option<String>, PersistError> {
        crate::metrics::time_db("fetch_open_turn_prompt_as_of", async {
            let prompt = sqlx::query_scalar!(
                "SELECT t.user_prompt
                   FROM memory.operator_turns t
                  WHERE t.session_id = $1
                    AND t.created_at <= $2
                    AND (t.closed_at IS NULL OR t.closed_at > $2)
                  ORDER BY t.id DESC
                  LIMIT 1",
                session_db_id,
                as_of,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(prompt.flatten())
        })
        .await
    }

    pub async fn fetch_acceptance_criteria_as_of(
        &self,
        session_db_id: i64,
        as_of: DateTime<Utc>,
    ) -> Result<Option<AcceptanceCriteriaMap>, PersistError> {
        crate::metrics::time_db("fetch_acceptance_criteria_as_of", async {
            let map = sqlx::query_as!(
                CriteriaMapHead,
                "SELECT id, version, ask_entry_count, model, output_tokens, created_at
                   FROM memory.acceptance_criteria_maps
                  WHERE session_id = $1
                    AND created_at <= $2
                  ORDER BY version DESC
                  LIMIT 1",
                session_db_id,
                as_of,
            )
            .fetch_optional(&self.pool)
            .await?;
            let Some(head) = map else {
                return Ok(None);
            };
            let criteria = sqlx::query_as!(
                AcceptanceCriterionRow,
                "SELECT ord, criterion, provenance, operator_span
                   FROM memory.acceptance_criteria
                  WHERE map_id = $1
                  ORDER BY ord ASC",
                head.id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(Some(AcceptanceCriteriaMap {
                id: head.id,
                version: head.version,
                ask_entry_count: head.ask_entry_count,
                model: head.model,
                output_tokens: head.output_tokens,
                created_at: head.created_at,
                criteria,
            }))
        })
        .await
    }
}
