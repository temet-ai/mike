use chrono::{DateTime, Utc};
use sqlx::postgres::PgArguments;
use sqlx::{Arguments, Postgres, QueryBuilder};

use super::super::{PersistError, PersistHandle};
use crate::store::persistence::operator_turns::eligibility::{
    BACKOFF_ELAPSED, eligible_without_verdict,
};

#[derive(Debug, Clone, PartialEq, sqlx::FromRow)]
pub struct JudgeTurnContextRow {
    pub turn_id: i64,
    pub session_id: i64,
    pub user_prompt: String,
    pub final_assistant_text: String,
    pub tool_call_count: i32,
    pub tool_error_count: i32,
    pub closed_at: DateTime<Utc>,
    pub next_state: String,
    pub next_genuine_request_id: Option<i64>,
    pub next_genuine_text: Option<String>,
    pub next_genuine_at: Option<DateTime<Utc>>,
    pub text_cut_at: i32,
    pub prompt_kind: String,
    pub session_class: String,
    pub stop_reason: Option<String>,
    pub aborted: bool,
    pub checks_count: i32,
    pub checks_failed: i32,
    pub checks_run: serde_json::Value,
    pub ended_on: String,
    pub files_changed: i32,
}

const CONTEXT_COLUMNS: &str = "SELECT t.id AS turn_id, t.session_id, \
        COALESCE(t.user_prompt, '') AS user_prompt, \
        COALESCE(t.final_assistant_text, '') AS final_assistant_text, \
        COALESCE(t.tool_call_count, 0)::int AS tool_call_count, \
        COALESCE(t.tool_error_count, 0)::int AS tool_error_count, \
        t.closed_at, f.next_state, f.next_genuine_request_id, \
        COALESCE(nt.user_prompt, nr.human_prompt) AS next_genuine_text, \
        f.next_genuine_at, f.text_cut_at, f.prompt_kind, f.session_class, f.stop_reason, \
        f.aborted, f.checks_count, f.checks_failed, f.checks_run, f.ended_on, \
        COALESCE(cardinality(t.files_touched), 0)::int AS files_changed";

const CONTEXT_JOINS: &str = "FROM memory.operator_turns t \
   JOIN memory.turn_facts f ON f.operator_turn_id = t.id \
   LEFT JOIN memory.operator_turns nt ON nt.id = f.next_genuine_turn_id \
   LEFT JOIN requests.requests nr ON nr.id = f.next_genuine_request_id";

impl PersistHandle {
    pub async fn fetch_judge_turn_contexts(
        &self,
        turn_ids: &[i64],
    ) -> Result<Vec<JudgeTurnContextRow>, PersistError> {
        if turn_ids.is_empty() {
            return Ok(Vec::new());
        }
        crate::metrics::time_db("fetch_judge_turn_contexts", async move {
            let mut query = QueryBuilder::<Postgres>::new(CONTEXT_COLUMNS);
            query
                .push(" ")
                .push(CONTEXT_JOINS)
                .push(" WHERE t.id = ANY(")
                .push_bind(turn_ids)
                .push(") AND t.closed_at IS NOT NULL ORDER BY t.id");
            let rows = query
                .build_query_as::<JudgeTurnContextRow>()
                .fetch_all(&self.pool)
                .await?;
            Ok(rows)
        })
        .await
    }

    pub(crate) async fn eligible_judge_turn_context(
        &self,
        bundle_id: i64,
        turn_id: i64,
    ) -> Result<Option<JudgeTurnContextRow>, PersistError> {
        crate::metrics::time_db("eligible_judge_turn_context", async move {
            let mut arguments = PgArguments::default();
            arguments.add(bundle_id).map_err(sqlx::Error::Encode)?;
            let mut query = QueryBuilder::<Postgres>::with_arguments(CONTEXT_COLUMNS, arguments);
            query
                .push(" ")
                .push(CONTEXT_JOINS)
                .push(" WHERE t.id = ")
                .push_bind(turn_id)
                .push(" AND t.id IN (SELECT t.id ")
                .push(eligible_without_verdict())
                .push(" ")
                .push(BACKOFF_ELAPSED)
                .push(" AND t.id = ")
                .push_bind(turn_id)
                .push(")");
            Ok(query
                .build_query_as::<JudgeTurnContextRow>()
                .fetch_optional(&self.pool)
                .await?)
        })
        .await
    }
}
