use sqlx::PgPool;

use crate::store::persistence::PersistError;

#[derive(Debug)]
pub(crate) struct WorkflowRunRow {
    pub(crate) use_id: i64,
    pub(crate) parent_request_id: i64,
    pub(crate) parent_created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) script: Option<String>,
    pub(crate) result_text: Option<String>,
}

#[derive(Debug)]
pub(crate) struct WorkflowAgentRow {
    pub(crate) agent_id: String,
    pub(crate) request_id: i64,
    pub(crate) req_created_at: chrono::DateTime<chrono::Utc>,
    pub(crate) subagent_persona: Option<String>,
    pub(crate) result_json: Option<serde_json::Value>,
}

pub(crate) async fn fetch_workflow_runs(
    pool: &PgPool,
    session_db_id: i64,
) -> Result<Vec<WorkflowRunRow>, PersistError> {
    let rows = sqlx::query_as!(
        WorkflowRunRow,
        r#"SELECT u.id                AS "use_id!",
                  u.request_id        AS "parent_request_id!",
                  r.created_at        AS "parent_created_at!",
                  u.input->>'script'  AS "script?",
                  tr.content#>>'{}'   AS "result_text?"
             FROM tools.uses u
             JOIN requests.requests r ON r.id = u.request_id
             LEFT JOIN tools.results tr ON tr.tool_use_id = u.id
            WHERE u.session_id = $1
              AND u.tool_name = 'Workflow'
            ORDER BY r.created_at ASC"#,
        session_db_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub(crate) async fn fetch_workflow_agents(
    pool: &PgPool,
    session_db_id: i64,
) -> Result<Vec<WorkflowAgentRow>, PersistError> {
    let rows = sqlx::query_file_as!(
        WorkflowAgentRow,
        "src/store/persistence/rehydrate/workflow_agents.sql",
        session_db_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
