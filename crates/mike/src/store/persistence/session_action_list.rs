use super::PersistError;

#[derive(Debug)]
pub struct SessionActionRow {
    pub tool_name: String,
    pub file_path: Option<String>,
    pub command: Option<String>,
    pub has_result: bool,
    pub result_is_error: Option<bool>,
    pub operation: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SessionActionCounts {
    pub total: i64,
    pub reads: i64,
    pub writes: i64,
    pub commands: i64,
    pub failed_commands: i64,
}

impl crate::store::persistence::steerer_repo::SteererDb<'_> {
    pub async fn fetch_session_action_list(
        self,
        session_db_id: i64,
    ) -> Result<Vec<SessionActionRow>, PersistError> {
        crate::metrics::time_db("fetch_session_action_list", async {
            let rows = sqlx::query_as!(
                SessionActionRow,
                r#"SELECT tu.tool_name,
                          tu.file_path,
                          CASE WHEN tu.tool_name = 'Bash'
                               THEN tu.input ->> 'command'
                          END AS "command?",
                          (tr.tool_use_id IS NOT NULL) AS "has_result!",
                          tr.is_error AS "result_is_error?",
                          tu.operation
                     FROM tools.uses tu
                     LEFT JOIN tools.results tr ON tr.tool_use_id = tu.id
                    WHERE tu.session_id = $1
                    ORDER BY tu.created_at ASC, tu.ord ASC"#,
                session_db_id,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn fetch_session_action_list_recent(
        self,
        session_db_id: i64,
        limit: i64,
    ) -> Result<Vec<SessionActionRow>, PersistError> {
        crate::metrics::time_db("fetch_session_action_list_recent", async {
            let rows = sqlx::query_as!(
                SessionActionRow,
                r#"SELECT x.tool_name AS "tool_name!", x.file_path, x.command AS "command?",
                          x.has_result AS "has_result!", x.result_is_error AS "result_is_error?",
                          x.operation
                     FROM (
                          SELECT tu.tool_name,
                                 tu.file_path,
                                 CASE WHEN tu.tool_name = 'Bash'
                                      THEN tu.input ->> 'command'
                                 END AS command,
                                 (tr.tool_use_id IS NOT NULL) AS has_result,
                                 tr.is_error AS result_is_error,
                                 tu.operation,
                                 tu.created_at,
                                 tu.ord
                            FROM tools.uses tu
                            LEFT JOIN tools.results tr ON tr.tool_use_id = tu.id
                           WHERE tu.session_id = $1
                           ORDER BY tu.created_at DESC, tu.ord DESC
                           LIMIT $2) x
                    ORDER BY x.created_at ASC, x.ord ASC"#,
                session_db_id,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn fetch_session_action_counts(
        self,
        session_db_id: i64,
    ) -> Result<SessionActionCounts, PersistError> {
        crate::metrics::time_db("fetch_session_action_counts", async {
            let counts = sqlx::query_as!(
                SessionActionCounts,
                r#"SELECT count(*) AS "total!",
                          count(*) FILTER (WHERE tu.operation IN ('read', 'search', 'fetch')) AS "reads!",
                          count(*) FILTER (WHERE tu.operation = 'write') AS "writes!",
                          count(*) FILTER (WHERE tu.tool_name = 'Bash') AS "commands!",
                          count(*) FILTER (WHERE tu.tool_name = 'Bash' AND tr.is_error) AS "failed_commands!"
                     FROM tools.uses tu
                     LEFT JOIN tools.results tr ON tr.tool_use_id = tu.id
                    WHERE tu.session_id = $1"#,
                session_db_id,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(counts)
        })
        .await
    }
}
