use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;
use uuid::Uuid;

#[derive(Debug, Clone, Default)]
pub(crate) struct ThreadFact {
    pub thread_id: String,
    pub root_thread_id: String,
    pub parent_thread_id: Option<String>,
    pub agent_type: Option<String>,
    pub cwd: Option<String>,
    pub model: Option<String>,
    pub codex_version: Option<String>,
    pub originator: Option<String>,
    pub ended_reason: Option<String>,
    pub source: &'static str,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct TurnFact {
    pub turn_id: String,
    pub thread_id: String,
    pub prompt_submitted: bool,
    pub requests_started: i32,
    pub requests_finished: i32,
    pub stopped: bool,
    pub last_assistant_message: Option<String>,
    pub source: &'static str,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct ToolFact {
    pub call_id: String,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub kind: &'static str,
    pub hook_tool_name: Option<String>,
    pub telemetry_tool_name: Option<String>,
    pub telemetry_namespace: Option<String>,
    pub input: Option<Value>,
    pub process_session: Option<String>,
    pub started: bool,
    pub completed: bool,
    pub duration_ms: Option<i64>,
    pub success: Option<bool>,
    pub exit_code: Option<i32>,
    pub hook_response_seen: bool,
    pub telemetry_output_snippet: bool,
    pub source: &'static str,
}

#[derive(Debug, Clone, Default)]
pub(crate) struct RequestFact {
    pub source: &'static str,
    pub phase: &'static str,
    pub thread_id: Option<String>,
    pub turn_id: Option<String>,
    pub endpoint: Option<String>,
    pub attempt: Option<i32>,
    pub status: Option<String>,
    pub http_status: Option<i32>,
    pub success: Option<bool>,
    pub error: Option<String>,
    pub duration_ms: Option<i64>,
    pub usage: Option<Value>,
}

fn json_text(value: Option<&Value>) -> Result<Option<String>, serde_json::Error> {
    value.map(serde_json::to_string).transpose()
}

pub(crate) async fn upsert_thread(
    pool: &PgPool,
    namespace_id: Uuid,
    fact: &ThreadFact,
    at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    let kind = if fact.parent_thread_id.is_some() {
        "subagent"
    } else {
        "session"
    };
    let ended_at = fact.ended_reason.as_ref().map(|_| at);
    sqlx::query(
        r#"INSERT INTO runtime.codex_observed_threads AS t
             (namespace_id, thread_id, root_thread_id, parent_thread_id, kind, agent_type, cwd,
              model, codex_version, originator, first_seen_at, last_seen_at, ended_at, end_reason,
              sources)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11, $12, $13, ARRAY[$14::text])
           ON CONFLICT (namespace_id, thread_id) DO UPDATE SET
             parent_thread_id = COALESCE(t.parent_thread_id, EXCLUDED.parent_thread_id),
             root_thread_id = CASE WHEN t.parent_thread_id IS NULL AND EXCLUDED.parent_thread_id IS NOT NULL
                                   THEN EXCLUDED.root_thread_id ELSE t.root_thread_id END,
             kind = CASE WHEN COALESCE(t.parent_thread_id, EXCLUDED.parent_thread_id) IS NULL
                         THEN 'session' ELSE 'subagent' END,
             agent_type = COALESCE(EXCLUDED.agent_type, t.agent_type),
             cwd = COALESCE(EXCLUDED.cwd, t.cwd),
             model = COALESCE(EXCLUDED.model, t.model),
             codex_version = COALESCE(EXCLUDED.codex_version, t.codex_version),
             originator = COALESCE(EXCLUDED.originator, t.originator),
             first_seen_at = LEAST(t.first_seen_at, EXCLUDED.first_seen_at),
             last_seen_at = GREATEST(t.last_seen_at, EXCLUDED.last_seen_at),
             ended_at = COALESCE(t.ended_at, EXCLUDED.ended_at),
             end_reason = COALESCE(t.end_reason, EXCLUDED.end_reason),
             sources = ARRAY(SELECT DISTINCT s FROM unnest(t.sources || EXCLUDED.sources) AS s ORDER BY s)"#,
    )
    .bind(namespace_id)
    .bind(&fact.thread_id)
    .bind(&fact.root_thread_id)
    .bind(&fact.parent_thread_id)
    .bind(kind)
    .bind(&fact.agent_type)
    .bind(&fact.cwd)
    .bind(&fact.model)
    .bind(&fact.codex_version)
    .bind(&fact.originator)
    .bind(at)
    .bind(ended_at)
    .bind(&fact.ended_reason)
    .bind(fact.source)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn upsert_turn(
    pool: &PgPool,
    namespace_id: Uuid,
    fact: &TurnFact,
    at: DateTime<Utc>,
) -> Result<(), sqlx::Error> {
    let stopped_at = fact.stopped.then_some(at);
    sqlx::query(
        r#"INSERT INTO runtime.codex_observed_turns AS t
             (namespace_id, turn_id, thread_id, prompt_submitted, model_requests_started,
              model_requests_finished, first_seen_at, stopped_at, last_assistant_message, sources)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, ARRAY[$10::text])
           ON CONFLICT (namespace_id, turn_id) DO UPDATE SET
             prompt_submitted = t.prompt_submitted OR EXCLUDED.prompt_submitted,
             model_requests_started = t.model_requests_started + EXCLUDED.model_requests_started,
             model_requests_finished = t.model_requests_finished + EXCLUDED.model_requests_finished,
             first_seen_at = LEAST(t.first_seen_at, EXCLUDED.first_seen_at),
             stopped_at = GREATEST(t.stopped_at, EXCLUDED.stopped_at),
             last_assistant_message = COALESCE(EXCLUDED.last_assistant_message, t.last_assistant_message),
             sources = ARRAY(SELECT DISTINCT s FROM unnest(t.sources || EXCLUDED.sources) AS s ORDER BY s)"#,
    )
    .bind(namespace_id)
    .bind(&fact.turn_id)
    .bind(&fact.thread_id)
    .bind(fact.prompt_submitted)
    .bind(fact.requests_started)
    .bind(fact.requests_finished)
    .bind(at)
    .bind(stopped_at)
    .bind(&fact.last_assistant_message)
    .bind(fact.source)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn upsert_tool(
    pool: &PgPool,
    namespace_id: Uuid,
    fact: &ToolFact,
    at: DateTime<Utc>,
) -> Result<(), StoreError> {
    let started_at = fact.started.then_some(at);
    let completed_at = fact.completed.then_some(at);
    sqlx::query(
        r#"INSERT INTO runtime.codex_observed_tools AS t
             (namespace_id, call_id, thread_id, turn_id, kind, hook_tool_name, telemetry_tool_name,
              telemetry_namespace, input, process_session, started_at, completed_at, duration_ms,
              success, exit_code, hook_response_seen, telemetry_output_snippet, sources)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9::text::json, $10, $11, $12, $13, $14, $15,
                   $16, $17, ARRAY[$18::text])
           ON CONFLICT (namespace_id, call_id) DO UPDATE SET
             thread_id = COALESCE(t.thread_id, EXCLUDED.thread_id),
             turn_id = COALESCE(t.turn_id, EXCLUDED.turn_id),
             kind = CASE WHEN t.kind = 'other' THEN EXCLUDED.kind ELSE t.kind END,
             hook_tool_name = COALESCE(t.hook_tool_name, EXCLUDED.hook_tool_name),
             telemetry_tool_name = COALESCE(t.telemetry_tool_name, EXCLUDED.telemetry_tool_name),
             telemetry_namespace = COALESCE(t.telemetry_namespace, EXCLUDED.telemetry_namespace),
             input = COALESCE(t.input, EXCLUDED.input),
             process_session = COALESCE(t.process_session, EXCLUDED.process_session),
             started_at = LEAST(t.started_at, EXCLUDED.started_at),
             completed_at = GREATEST(t.completed_at, EXCLUDED.completed_at),
             duration_ms = COALESCE(t.duration_ms, EXCLUDED.duration_ms),
             success = COALESCE(EXCLUDED.success, t.success),
             exit_code = COALESCE(EXCLUDED.exit_code, t.exit_code),
             hook_response_seen = t.hook_response_seen OR EXCLUDED.hook_response_seen,
             telemetry_output_snippet = t.telemetry_output_snippet OR EXCLUDED.telemetry_output_snippet,
             sources = ARRAY(SELECT DISTINCT s FROM unnest(t.sources || EXCLUDED.sources) AS s ORDER BY s)"#,
    )
    .bind(namespace_id)
    .bind(&fact.call_id)
    .bind(&fact.thread_id)
    .bind(&fact.turn_id)
    .bind(fact.kind)
    .bind(&fact.hook_tool_name)
    .bind(&fact.telemetry_tool_name)
    .bind(&fact.telemetry_namespace)
    .bind(json_text(fact.input.as_ref())?)
    .bind(&fact.process_session)
    .bind(started_at)
    .bind(completed_at)
    .bind(fact.duration_ms)
    .bind(fact.success)
    .bind(fact.exit_code)
    .bind(fact.hook_response_seen)
    .bind(fact.telemetry_output_snippet)
    .bind(fact.source)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn insert_request(
    pool: &PgPool,
    namespace_id: Uuid,
    ingestion_id: Uuid,
    ordinal: i32,
    fact: &RequestFact,
    at: DateTime<Utc>,
) -> Result<(), StoreError> {
    sqlx::query(
        r#"INSERT INTO runtime.codex_observed_requests
             (namespace_id, ingestion_id, ordinal, source, phase, thread_id, turn_id, endpoint,
              attempt, status, http_status, success, error, duration_ms, usage, observed_at)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15::text::json, $16)
           ON CONFLICT (namespace_id, ingestion_id, ordinal) DO NOTHING"#,
    )
    .bind(namespace_id)
    .bind(ingestion_id)
    .bind(ordinal)
    .bind(fact.source)
    .bind(fact.phase)
    .bind(&fact.thread_id)
    .bind(&fact.turn_id)
    .bind(&fact.endpoint)
    .bind(fact.attempt)
    .bind(&fact.status)
    .bind(fact.http_status)
    .bind(fact.success)
    .bind(&fact.error)
    .bind(fact.duration_ms)
    .bind(json_text(fact.usage.as_ref())?)
    .bind(at)
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn insert_hook_run(
    pool: &PgPool,
    namespace_id: Uuid,
    run: &super::otel::HookRun,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        r#"INSERT INTO runtime.codex_observed_hook_runs
             (namespace_id, trace_id, span_id, event_name, outcome, execution_mode, timeout_sec,
              started_at, duration_ms)
           VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
           ON CONFLICT (namespace_id, trace_id, span_id) DO NOTHING"#,
    )
    .bind(namespace_id)
    .bind(&run.trace_id)
    .bind(&run.span_id)
    .bind(&run.event_name)
    .bind(&run.outcome)
    .bind(&run.execution_mode)
    .bind(run.timeout_sec)
    .bind(run.started_at)
    .bind(run.duration_ms)
    .execute(pool)
    .await?;
    Ok(())
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum StoreError {
    #[error("database: {0}")]
    Database(#[from] sqlx::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}
