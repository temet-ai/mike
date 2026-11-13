use super::{PersistError, PersistHandle};

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct ToolNameStatRow {
    pub tool_name: String,
    pub use_count: i64,
    pub total_result_bytes: i64,
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct DuplicateInputGroupRow {
    pub tool_name: String,
    pub input_digest: String,
    pub use_count: i64,
    pub avg_result_bytes: f64,
    pub sample_request_ids: Vec<i64>,
    pub input_excerpt: Option<String>,
}

#[derive(Debug, sqlx::FromRow, serde::Serialize)]
pub struct TopResultRow {
    pub tool_name: String,
    pub request_id: i64,
    pub tool_id: String,
    pub content_bytes: i64,
    pub input_excerpt: Option<String>,
}

impl PersistHandle {
    pub async fn fetch_session_tool_stats_by_tool(
        &self,
        session_db_id: i64,
    ) -> Result<Vec<ToolNameStatRow>, PersistError> {
        crate::metrics::time_db("fetch_session_tool_stats_by_tool", async {
            let rows = sqlx::query_as!(
                ToolNameStatRow,
                r#"SELECT tu.tool_name AS "tool_name!",
                        COUNT(*)::bigint AS "use_count!",
                        COALESCE(SUM(tr.content_bytes), 0)::bigint AS "total_result_bytes!"
                   FROM tools.uses tu
                   LEFT JOIN tools.results tr ON tr.tool_use_id = tu.id
                  WHERE tu.session_id = $1
                  GROUP BY tu.tool_name
                  ORDER BY 3 DESC, 2 DESC"#,
                session_db_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn fetch_session_duplicate_input_groups(
        &self,
        session_db_id: i64,
    ) -> Result<Vec<DuplicateInputGroupRow>, PersistError> {
        crate::metrics::time_db("fetch_session_duplicate_input_groups", async {
            let rows = sqlx::query_as!(
                DuplicateInputGroupRow,
                r#"SELECT tu.tool_name AS "tool_name!",
                        md5(tu.input::text) AS "input_digest!",
                        COUNT(*)::bigint AS "use_count!",
                        COALESCE(AVG(tr.content_bytes), 0)::float8 AS "avg_result_bytes!",
                        (ARRAY_AGG(tu.request_id ORDER BY tu.request_id))[1:6] AS "sample_request_ids!",
                        LEFT(MIN(tu.input::text), 120) AS "input_excerpt?"
                   FROM tools.uses tu
                   LEFT JOIN tools.results tr ON tr.tool_use_id = tu.id
                  WHERE tu.session_id = $1
                    AND tu.input IS NOT NULL
                  GROUP BY tu.tool_name, md5(tu.input::text)
                 HAVING COUNT(*) > 1
                  ORDER BY 3 DESC, 4 DESC"#,
                session_db_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn fetch_session_top_results(
        &self,
        session_db_id: i64,
        limit: i64,
    ) -> Result<Vec<TopResultRow>, PersistError> {
        crate::metrics::time_db("fetch_session_top_results", async {
            let rows = sqlx::query_as!(
                TopResultRow,
                r#"SELECT tu.tool_name AS "tool_name!",
                        tu.request_id AS "request_id!",
                        tu.wire_tool_use_id AS "tool_id!",
                        tr.content_bytes AS "content_bytes!",
                        LEFT(tu.input::text, 120) AS "input_excerpt?"
                   FROM tools.results tr
                   JOIN tools.uses tu ON tu.id = tr.tool_use_id
                  WHERE tr.session_id = $1
                  ORDER BY tr.content_bytes DESC
                  LIMIT $2"#,
                session_db_id,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }
}
