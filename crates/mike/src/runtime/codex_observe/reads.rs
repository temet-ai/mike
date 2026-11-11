use chrono::{DateTime, Utc};
use serde::Serialize;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, PartialEq, sqlx::FromRow)]
pub(crate) struct ObservedThread {
    pub namespace_id: Uuid,
    pub thread_id: String,
    pub root_thread_id: String,
    pub parent_thread_id: Option<String>,
    pub kind: String,
    pub agent_type: Option<String>,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub codex_version: Option<String>,
    pub first_seen_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
    pub end_reason: Option<String>,
    pub sources: Vec<String>,
    pub turns: i64,
    pub tools: i64,
    pub tools_seen_by_hook_and_telemetry: i64,
    pub hook_model_requests: i64,
    pub telemetry_request_records: i64,
    pub managed_by_mike: bool,
}

pub(crate) async fn list_threads(
    pool: &PgPool,
    root_thread_id: Option<&str>,
    limit: i64,
    offset: i64,
) -> Result<Vec<ObservedThread>, sqlx::Error> {
    sqlx::query_as::<_, ObservedThread>(
        r#"WITH page AS (
             SELECT * FROM runtime.codex_observed_threads
              WHERE $1::text IS NULL OR root_thread_id = $1
              ORDER BY first_seen_at DESC, thread_id
              LIMIT $2 OFFSET $3
           ), turns AS (
             SELECT u.namespace_id, u.thread_id, count(*) AS n
               FROM runtime.codex_observed_turns u JOIN page p USING (namespace_id, thread_id)
              GROUP BY u.namespace_id, u.thread_id
           ), tools AS (
             SELECT o.namespace_id, o.thread_id, count(*) AS n,
                    count(*) FILTER (WHERE o.sources @> ARRAY['hook', 'telemetry']) AS both_sources
               FROM runtime.codex_observed_tools o JOIN page p USING (namespace_id, thread_id)
              GROUP BY o.namespace_id, o.thread_id
           ), requests AS (
             SELECT r.namespace_id, r.thread_id,
                    count(*) FILTER (WHERE r.source = 'hook' AND r.phase = 'finished') AS hook_n,
                    count(*) FILTER (WHERE r.source = 'telemetry') AS telemetry_n
               FROM runtime.codex_observed_requests r JOIN page p USING (namespace_id, thread_id)
              GROUP BY r.namespace_id, r.thread_id
           )
           SELECT p.namespace_id AS namespace_id, p.thread_id AS thread_id,
                  p.root_thread_id AS root_thread_id, p.parent_thread_id, p.kind AS kind,
                  p.agent_type, p.cwd, p.model, p.codex_version,
                  p.first_seen_at AS first_seen_at, p.last_seen_at AS last_seen_at,
                  p.ended_at, p.end_reason, p.sources AS sources,
                  COALESCE(turns.n, 0) AS turns, COALESCE(tools.n, 0) AS tools,
                  COALESCE(tools.both_sources, 0) AS tools_seen_by_hook_and_telemetry,
                  COALESCE(requests.hook_n, 0) AS hook_model_requests,
                  COALESCE(requests.telemetry_n, 0) AS telemetry_request_records,
                  EXISTS (SELECT 1 FROM runtime.threads rt
                           WHERE rt.provider_thread_id = p.thread_id) AS managed_by_mike
             FROM page p
             LEFT JOIN turns USING (namespace_id, thread_id)
             LEFT JOIN tools USING (namespace_id, thread_id)
             LEFT JOIN requests USING (namespace_id, thread_id)
            ORDER BY p.first_seen_at DESC, p.thread_id"#,
    )
    .bind(root_thread_id)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await
}

#[derive(Debug, Clone, Serialize, PartialEq, sqlx::FromRow)]
pub(crate) struct ObservedTool {
    pub call_id: String,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub kind: String,
    pub hook_tool_name: Option<String>,
    pub telemetry_tool_name: Option<String>,
    pub process_session: Option<String>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub success: Option<bool>,
    pub exit_code: Option<i32>,
    pub hook_response_seen: bool,
    pub telemetry_output_snippet: bool,
    pub sources: Vec<String>,
}

pub(crate) async fn list_tools(
    pool: &PgPool,
    namespace_id: Uuid,
    thread_ids: &[String],
) -> Result<Vec<ObservedTool>, sqlx::Error> {
    sqlx::query_as::<_, ObservedTool>(
        r#"SELECT call_id AS call_id, thread_id, turn_id, kind AS kind, hook_tool_name,
                  telemetry_tool_name, process_session, started_at, completed_at, duration_ms,
                  success, exit_code, hook_response_seen AS hook_response_seen,
                  telemetry_output_snippet AS telemetry_output_snippet, sources AS sources
             FROM runtime.codex_observed_tools
            WHERE namespace_id = $1 AND thread_id = ANY($2)
            ORDER BY COALESCE(started_at, completed_at), call_id"#,
    )
    .bind(namespace_id)
    .bind(thread_ids)
    .fetch_all(pool)
    .await
}
