use chrono::{DateTime, Utc};
use serde::Serialize;

use super::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct LedgerCandidateRow {
    pub turn_id: i64,
    pub session_id: i64,
    pub start_request_id: i64,
    pub user_prompt: String,
    pub created_at: DateTime<Utc>,
    pub has_earlier: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BuilderLedgerRow {
    pub session_id: i64,
    pub operator_turn_id: i64,
    pub request_id: i64,
    pub kind: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, sqlx::FromRow)]
pub struct IntakeRecount {
    pub map_id: i64,
    pub session_id: i64,
    pub old_count: i32,
    pub new_count: i32,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct HistoryLinkOutcome {
    pub rows_linked: u64,
    pub rows_unmatched: u64,
}

impl PersistHandle {
    pub async fn load_ledger_candidates(
        &self,
        session_ids: &[i64],
        since: DateTime<Utc>,
    ) -> Result<Vec<LedgerCandidateRow>, PersistError> {
        crate::metrics::time_db("load_ledger_candidates", async move {
            let rows = sqlx::query_file_as!(
                LedgerCandidateRow,
                "src/store/persistence/ask_ledger_backfill/candidates.sql",
                session_ids,
                since,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn insert_builder_ledger_rows(
        &self,
        rows: &[BuilderLedgerRow],
    ) -> Result<u64, PersistError> {
        if rows.is_empty() {
            return Ok(0);
        }
        let payload = serde_json::to_value(rows)?;
        crate::metrics::time_db("insert_builder_ledger_rows", async move {
            let done = sqlx::query!(
                "INSERT INTO memory.ask_ledger_entries
        (session_id, operator_turn_id, request_id, kind, kind_source, keyword_kind, content, created_at)
 SELECT x.session_id, x.operator_turn_id, x.request_id, x.kind, 'keyword', x.kind, x.content, x.created_at
   FROM jsonb_to_recordset($1::jsonb) AS x(
        session_id bigint, operator_turn_id bigint, request_id bigint,
        kind text, content text, created_at timestamptz)
 ON CONFLICT (request_id) WHERE request_id IS NOT NULL DO NOTHING",
                &payload,
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }

    pub async fn link_ask_ledger_to_turns(&self, session_ids: &[i64]) -> Result<u64, PersistError> {
        if session_ids.is_empty() {
            return Ok(0);
        }
        crate::metrics::time_db("link_ask_ledger_to_turns", async move {
            let done = sqlx::query!(
                "UPDATE memory.ask_ledger_entries e
    SET operator_turn_id = t.id
   FROM memory.operator_turns t
  WHERE t.start_request_id = e.request_id
    AND e.operator_turn_id IS NULL
    AND e.session_id = ANY($1)",
                session_ids,
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected())
        })
        .await
    }

    pub async fn link_ask_ledger_history(
        &self,
        session_ids: &[i64],
        execute: bool,
    ) -> Result<HistoryLinkOutcome, PersistError> {
        crate::metrics::time_db("link_ask_ledger_history", async move {
            let rows_linked = if execute {
                sqlx::query_file!(
                    "src/store/persistence/ask_ledger_backfill/link_history_update.sql",
                    session_ids,
                )
                .execute(&self.pool)
                .await?
                .rows_affected()
            } else {
                let n = sqlx::query_file_scalar!(
                    "src/store/persistence/ask_ledger_backfill/link_history_count.sql",
                    session_ids,
                )
                .fetch_one(&self.pool)
                .await?;
                u64::try_from(n).unwrap_or(0)
            };
            let unmatched = sqlx::query_scalar!(
                r#"SELECT count(*) AS "unmatched!" FROM memory.ask_ledger_entries
  WHERE request_id IS NULL AND session_id = ANY($1)"#,
                session_ids,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(HistoryLinkOutcome {
                rows_linked,
                rows_unmatched: u64::try_from(unmatched)
                    .unwrap_or(0)
                    .saturating_sub(if execute { 0 } else { rows_linked }),
            })
        })
        .await
    }

    pub async fn recount_intake_maps(
        &self,
        session_ids: &[i64],
        cap: i64,
        execute: bool,
    ) -> Result<Vec<IntakeRecount>, PersistError> {
        crate::metrics::time_db("recount_intake_maps", async move {
            let rows = if execute {
                sqlx::query_file_as!(
                    IntakeRecount,
                    "src/store/persistence/ask_ledger_backfill/recount_apply.sql",
                    session_ids,
                    cap,
                )
                .fetch_all(&self.pool)
                .await?
            } else {
                sqlx::query_file_as!(
                    IntakeRecount,
                    "src/store/persistence/ask_ledger_backfill/recount_preview.sql",
                    session_ids,
                    cap,
                )
                .fetch_all(&self.pool)
                .await?
            };
            Ok(rows)
        })
        .await
    }
}
