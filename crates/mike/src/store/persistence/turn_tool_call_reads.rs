use super::PersistError;

#[derive(Clone, sqlx::FromRow)]
pub(crate) struct TurnToolCallMetaRow {
    pub(crate) tool_id: String,
    pub(crate) tool_name: String,
    pub(crate) input_bytes: Option<i32>,
    pub(crate) ord: i32,
    pub(crate) request_id: i64,
    pub(crate) has_result: bool,
    pub(crate) result_is_error: Option<bool>,
    pub(crate) result_content_bytes: Option<i64>,
    pub(crate) operation: Option<String>,
}

#[derive(sqlx::FromRow)]
pub(crate) struct TurnToolCallContentRow {
    pub(crate) input: Option<serde_json::Value>,
    pub(crate) result_is_error: Option<bool>,
    pub(crate) result_content: Option<serde_json::Value>,
    pub(crate) result_content_bytes: Option<i64>,
    pub(crate) result_content_sha256: Option<String>,
}

impl crate::store::persistence::steerer_repo::SteererDb<'_> {
    pub(crate) async fn fetch_turn_bounds(
        self,
        turn_id: i64,
        session_db_id: i64,
    ) -> Result<Option<(i64, i64)>, PersistError> {
        crate::metrics::time_db("fetch_turn_bounds", async {
            let row = sqlx::query!(
                "SELECT start_request_id, end_request_id
                   FROM memory.operator_turns
                  WHERE id = $1 AND session_id = $2",
                turn_id,
                session_db_id,
            )
            .fetch_optional(&self.pool)
            .await?
            .map(|row| (row.start_request_id, row.end_request_id));
            Ok(row)
        })
        .await
    }
    pub(crate) async fn fetch_turn_tool_calls_paired(
        self,
        turn_id: i64,
        session_db_id: i64,
    ) -> Result<Vec<TurnToolCallMetaRow>, PersistError> {
        crate::metrics::time_db("fetch_turn_tool_calls_paired", async {
            let rows = sqlx::query_file_as!(
                TurnToolCallMetaRow,
                "src/store/persistence/turn_tool_call_reads/paired.sql",
                turn_id,
                session_db_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
    pub(crate) async fn fetch_turn_tool_call_content(
        self,
        turn_id: i64,
        session_db_id: i64,
        tool_wire_id: &str,
    ) -> Result<Option<TurnToolCallContentRow>, PersistError> {
        crate::metrics::time_db("fetch_turn_tool_call_content", async {
            let row = sqlx::query_file_as!(
                TurnToolCallContentRow,
                "src/store/persistence/turn_tool_call_reads/content.sql",
                turn_id,
                session_db_id,
                tool_wire_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }
}
