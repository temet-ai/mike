use super::PersistError;

#[derive(sqlx::FromRow)]
pub struct ToolUseRow {
    pub tu_id: i64,
    pub request_id: i64,
    pub session_id: String,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub tool_id: String,
    pub tool_name: String,
    pub input: Option<serde_json::Value>,
    pub file_path: Option<String>,
    pub ord: i32,
    pub agent_instance_id: Option<i64>,
    pub agent_name: Option<String>,
    pub subagent_persona: Option<String>,
}

#[derive(sqlx::FromRow)]
pub struct BatchToolResultRow {
    pub tu_id: i64,
    pub request_id: i64,
    pub wire_tool_use_id: String,
    pub is_error: bool,
    pub content_sha256: String,
    pub content_bytes: i64,
    pub content: serde_json::Value,
}

#[derive(sqlx::FromRow)]
pub struct ToolResultRow {
    pub tool_use_id: String,
    pub is_error: bool,
    pub content_sha256: String,
    pub content_bytes: i64,
    pub content: serde_json::Value,
}

#[derive(sqlx::FromRow)]
pub struct PairedToolCallRow {
    pub tool_id: String,
    pub tool_name: String,
    pub input: Option<serde_json::Value>,
    pub ord: i32,
    pub result_tool_use_id: Option<String>,
    pub result_is_error: Option<bool>,
    pub result_content_sha256: Option<String>,
    pub result_content_bytes: Option<i64>,
    pub result_content: Option<serde_json::Value>,
}

impl crate::store::persistence::steerer_repo::SteererDb<'_> {
    pub async fn count_tool_uses_for_session(
        self,
        session_db_id: i64,
        agent: Option<&str>,
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db("count_tool_uses_for_session", async {
            let count = match agent {
                Some(agent) => {
                    sqlx::query_scalar!(
                        r#"SELECT COUNT(*) AS "count!"
                   FROM tools.uses tu
              LEFT JOIN requests.requests r ON r.id = tu.request_id
              LEFT JOIN agents.instances ai ON ai.id = r.agent_instance_id
              LEFT JOIN agents.agents a ON a.id = ai.agent_id
                  WHERE tu.session_id = $1
                    AND (a.name = $2 OR r.subagent_persona = $2)"#,
                        session_db_id,
                        agent,
                    )
                    .fetch_one(&self.pool)
                    .await?
                }
                None => {
                    sqlx::query_scalar!(
                        r#"SELECT COUNT(*) AS "count!" FROM tools.uses WHERE session_id = $1"#,
                        session_db_id,
                    )
                    .fetch_one(&self.pool)
                    .await?
                }
            };
            Ok(count)
        })
        .await
    }
    pub async fn fetch_tool_uses_for_session(
        self,
        session_db_id: i64,
        offset: i64,
        limit: i64,
        order: crate::types::SortOrder,
        agent: Option<&str>,
    ) -> Result<Vec<ToolUseRow>, PersistError> {
        crate::metrics::time_db("fetch_tool_uses_for_session", async {
            let rows = match (agent, order) {
                (None, crate::types::SortOrder::Asc) => {
                    sqlx::query_file_as!(
                        ToolUseRow,
                        "src/store/persistence/tool_call_reads/uses_page_asc.sql",
                        session_db_id,
                        offset,
                        limit,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
                (None, crate::types::SortOrder::Desc) => {
                    sqlx::query_file_as!(
                        ToolUseRow,
                        "src/store/persistence/tool_call_reads/uses_page_desc.sql",
                        session_db_id,
                        offset,
                        limit,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
                (Some(agent), crate::types::SortOrder::Asc) => {
                    sqlx::query_file_as!(
                        ToolUseRow,
                        "src/store/persistence/tool_call_reads/uses_page_agent_asc.sql",
                        session_db_id,
                        offset,
                        limit,
                        agent,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
                (Some(agent), crate::types::SortOrder::Desc) => {
                    sqlx::query_file_as!(
                        ToolUseRow,
                        "src/store/persistence/tool_call_reads/uses_page_agent_desc.sql",
                        session_db_id,
                        offset,
                        limit,
                        agent,
                    )
                    .fetch_all(&self.pool)
                    .await?
                }
            };
            Ok(rows)
        })
        .await
    }
    pub async fn request_exists(self, request_id: i64) -> Result<bool, PersistError> {
        crate::metrics::time_db("request_exists", async {
            let exists = sqlx::query_scalar!(
                r#"SELECT EXISTS (SELECT 1 FROM requests.requests WHERE id = $1) AS "exists!""#,
                request_id,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(exists)
        })
        .await
    }
    pub async fn fetch_tool_results_for_request(
        self,
        request_id: i64,
    ) -> Result<Vec<ToolResultRow>, PersistError> {
        crate::metrics::time_db("fetch_tool_results_for_request", async {
            let rows = sqlx::query_file_as!(
                ToolResultRow,
                "src/store/persistence/tool_call_reads/results_for_request.sql",
                request_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
    pub async fn fetch_request_tool_calls_paired(
        self,
        request_id: i64,
    ) -> Result<Vec<PairedToolCallRow>, PersistError> {
        crate::metrics::time_db("fetch_request_tool_calls_paired", async {
            let rows = sqlx::query_file_as!(
                PairedToolCallRow,
                "src/store/persistence/tool_call_reads/paired_for_request.sql",
                request_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
    pub async fn fetch_results_for_tool_use_ids(
        self,
        tu_ids: &[i64],
        session_db_id: i64,
    ) -> Result<Vec<BatchToolResultRow>, PersistError> {
        if tu_ids.is_empty() {
            return Ok(vec![]);
        }
        crate::metrics::time_db("fetch_results_for_tool_use_ids", async {
            let rows = sqlx::query_file_as!(
                BatchToolResultRow,
                "src/store/persistence/tool_call_reads/results_for_tool_use_ids.sql",
                tu_ids,
                session_db_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn first_request_carrying_result(
        self,
        wire_tool_use_id: &str,
    ) -> Result<Option<i64>, PersistError> {
        crate::metrics::time_db("first_request_carrying_result", async {
            let id = sqlx::query_scalar!(
                r#"SELECT tr.request_id AS "request_id!"
                   FROM tools.uses tu
                   JOIN tools.results tr ON tr.tool_use_id = tu.id
                  WHERE tu.wire_tool_use_id = $1
                  ORDER BY tr.request_id
                  LIMIT 1"#,
                wire_tool_use_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(id)
        })
        .await
    }
}
