use sqlx::QueryBuilder;
use sqlx::postgres::Postgres;

use super::{
    LIST_FOR_SESSION_LIMIT, OperatorTurnRow, SESSION_OVERVIEW_SELECT_BASE, SessionOverviewFilter,
    SessionOverviewTurnRow, TURN_SELECT_BASE, TriggerTurn,
};
use crate::store::persistence::{PersistError, PersistHandle};
use crate::types::SortOrder;

fn session_turns_query(
    base: &str,
    session_db_id: i64,
    filter: &SessionOverviewFilter,
    order: SortOrder,
    cap: i64,
    offset: i64,
) -> QueryBuilder<Postgres> {
    let mut query = QueryBuilder::new(base);
    query
        .push(" WHERE t.session_id = ")
        .push_bind(session_db_id);
    if let Some(kind) = filter.prompt_kind.as_deref() {
        query.push(" AND pk_kinds.code = ").push_bind(kind);
    }
    query
        .push(" ORDER BY t.start_request_id ")
        .push(order.sql())
        .push(" LIMIT ")
        .push_bind(cap + 1)
        .push(" OFFSET ")
        .push_bind(offset);
    query
}

impl PersistHandle {
    pub async fn list_for_session_bounded(
        &self,
        session_db_id: i64,
        order: SortOrder,
        filter: &SessionOverviewFilter,
    ) -> Result<(Vec<OperatorTurnRow>, bool), PersistError> {
        crate::metrics::time_db("list_operator_turns_for_session_bounded", async move {
            let cap = filter.limit.unwrap_or(LIST_FOR_SESSION_LIMIT);
            let offset = filter.offset.unwrap_or(0);

            let mut rows =
                session_turns_query(TURN_SELECT_BASE, session_db_id, filter, order, cap, offset)
                    .build_query_as::<OperatorTurnRow>()
                    .fetch_all(&self.pool)
                    .await?;
            let truncated = rows.len() > cap as usize;
            rows.truncate(cap as usize);
            metrics::histogram!("memory_persist_list_for_session_rows").record(rows.len() as f64);
            if truncated {
                metrics::counter!("memory_persist_list_for_session_truncated_total").increment(1);
            }
            Ok((rows, truncated))
        })
        .await
    }

    pub async fn session_overview_bounded(
        &self,
        session_db_id: i64,
        order: SortOrder,
        filter: &SessionOverviewFilter,
    ) -> Result<(Vec<SessionOverviewTurnRow>, i64, bool), PersistError> {
        crate::metrics::time_db("session_overview_bounded", async move {
            let total: i64 = sqlx::query_scalar!(
                r#"SELECT COUNT(*) AS "count!" FROM memory.operator_turns WHERE session_id = $1"#,
                session_db_id,
            )
            .fetch_one(&self.pool)
            .await?;

            let cap = filter.limit.unwrap_or(LIST_FOR_SESSION_LIMIT);
            let offset = filter.offset.unwrap_or(0);

            let mut rows = session_turns_query(
                SESSION_OVERVIEW_SELECT_BASE,
                session_db_id,
                filter,
                order,
                cap,
                offset,
            )
            .build_query_as::<SessionOverviewTurnRow>()
            .fetch_all(&self.pool)
            .await?;
            let truncated = rows.len() > cap as usize;
            rows.truncate(cap as usize);

            if filter.omit_stop_response {
                for row in &mut rows {
                    row.stop_response = None;
                }
            }

            Ok((rows, total, truncated))
        })
        .await
    }

    pub async fn get_by_id(&self, turn_id: i64) -> Result<Option<OperatorTurnRow>, PersistError> {
        crate::metrics::time_db("get_operator_turn_by_id", async move {
            let row = QueryBuilder::<Postgres>::new(TURN_SELECT_BASE)
                .push(" WHERE t.id = ")
                .push_bind(turn_id)
                .build_query_as::<OperatorTurnRow>()
                .fetch_optional(&self.pool)
                .await?;
            Ok(row)
        })
        .await
    }

    pub async fn get_open_turn_prompt(
        &self,
        session_uuid: uuid::Uuid,
    ) -> Result<Option<String>, PersistError> {
        crate::metrics::time_db("get_open_turn_prompt", async move {
            let row = sqlx::query!(
                r#"SELECT t.user_prompt
                     FROM memory.operator_turns t
                     JOIN sessions.sessions s ON s.id = t.session_id
                    WHERE s.session_uuid = $1 AND t.closed_at IS NULL
                    ORDER BY t.id DESC
                    LIMIT 1"#,
                session_uuid,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.and_then(|r| r.user_prompt))
        })
        .await
    }

    pub async fn get_turn_for_trigger_request(
        &self,
        session_uuid: uuid::Uuid,
        trigger_request_id: i64,
    ) -> Result<Option<TriggerTurn>, PersistError> {
        crate::metrics::time_db("get_turn_for_trigger_request", async move {
            let row = sqlx::query_as!(
                TriggerTurn,
                r#"SELECT s.id AS session_db_id,
                          COALESCE(r.operator_turn_id, latest.id) AS "operator_turn_id?",
                          r.claude_code_agent_id, r.agent_instance_id, r.subagent_persona
                     FROM requests.requests r
                     JOIN sessions.sessions s ON s.id = r.session_id
                     LEFT JOIN LATERAL (
                          SELECT t.id
                            FROM memory.operator_turns t
                           WHERE t.session_id = r.session_id
                             AND t.created_at <= r.created_at
                           ORDER BY t.created_at DESC
                           LIMIT 1
                     ) latest ON r.operator_turn_id IS NULL
                    WHERE r.id = $1
                      AND s.session_uuid = $2"#,
                trigger_request_id,
                session_uuid,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }

    pub async fn get_open_turn_ref(
        &self,
        session_uuid: uuid::Uuid,
    ) -> Result<Option<(i64, i64)>, PersistError> {
        crate::metrics::time_db("get_open_turn_ref", async move {
            let row = sqlx::query!(
                r#"SELECT t.id, t.end_request_id
                     FROM memory.operator_turns t
                     JOIN sessions.sessions s ON s.id = t.session_id
                    WHERE s.session_uuid = $1 AND t.closed_at IS NULL
                    ORDER BY t.id DESC
                    LIMIT 1"#,
                session_uuid,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.map(|r| (r.id, r.end_request_id)))
        })
        .await
    }

    pub async fn session_id_for_turn(&self, turn_id: i64) -> Result<Option<i64>, PersistError> {
        crate::metrics::time_db("session_id_for_turn", async move {
            let id: Option<i64> = sqlx::query_scalar!(
                "SELECT session_id FROM memory.operator_turns WHERE id = $1",
                turn_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(id)
        })
        .await
    }

    pub async fn latest_turn_id_for_session(
        &self,
        session_db_id: i64,
    ) -> Result<Option<i64>, PersistError> {
        crate::metrics::time_db("latest_turn_id_for_session", async move {
            let id: Option<i64> = sqlx::query_scalar!(
                "SELECT id FROM memory.operator_turns
                  WHERE session_id = $1
                  ORDER BY id DESC
                  LIMIT 1",
                session_db_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(id)
        })
        .await
    }
}
