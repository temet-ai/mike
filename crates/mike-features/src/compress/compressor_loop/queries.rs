use std::collections::{BTreeMap, BTreeSet};

use anyhow::{Context, Result};
use chrono::{DateTime, Duration, Utc};
use sqlx::{PgPool, Postgres, Transaction};

use super::stats::roll_up;
use super::types::{Arms, Call, Cut, Lane, SOURCE_BATCH, Source, Usage, UsageRow};

const STATEMENT_TIMEOUT_MS: &str = "300000";

const ARMS_SQL: &str = "
SELECT q.session_id, min(st->'resolved_parameters'->>'arm') AS arm,
       CASE WHEN min(st->'resolved_parameters'->>'arm') = max(st->'resolved_parameters'->>'arm')
            THEN 1 ELSE 2 END AS observed_arms
FROM requests.requests q
CROSS JOIN LATERAL jsonb_array_elements(q.transformation_manifest->'stages') st
WHERE q.created_at >= $1 AND q.created_at < $2
  AND q.transformation_manifest IS NOT NULL
  AND st->>'stage' = $3
  AND st->'resolved_parameters' ? 'arm'
GROUP BY q.session_id";

const CUTS_SQL: &str = "
WITH cut_rows AS MATERIALIZED (
  SELECT q.session_id, q.agent_instance_id, q.claude_code_agent_id, o.code AS origin,
         q.created_at, t->>'tool_use_id' AS wire_id,
         t->>'reason_code' AS reason, t->>'detail' AS detail
  FROM requests.requests q
  JOIN requests.turn_origins o ON o.id = q.turn_origin_id
  CROSS JOIN LATERAL jsonb_array_elements(q.transformation_manifest->'stages') st
  CROSS JOIN LATERAL jsonb_array_elements(st->'targets') t
  WHERE q.created_at >= $1 AND q.session_id = ANY($2::bigint[])
    AND st->>'stage' = $3
    AND t->>'reason_code' = ANY($4::text[])
    AND t->>'tool_use_id' IS NOT NULL
)
SELECT DISTINCT ON (session_id, agent_instance_id, claude_code_agent_id, wire_id)
       session_id, agent_instance_id, claude_code_agent_id, origin,
       created_at, wire_id, reason, detail
FROM cut_rows
ORDER BY session_id, agent_instance_id, claude_code_agent_id, wire_id, created_at";

const USAGE_SQL: &str = "
WITH billed AS MATERIALIZED (
  SELECT q.session_id, q.agent_instance_id, q.claude_code_agent_id, o.code AS origin,
         CASE WHEN r.id IS NULL THEN NULL
              WHEN r.token_usage <> '{}'::jsonb THEN r.token_usage
              WHEN length(r.response_body) = 0 THEN NULL
              WHEN get_byte(r.response_body, 0) <> 123 THEN NULL
              WHEN NOT pg_input_is_valid(convert_from(r.response_body, 'UTF8'), 'jsonb') THEN NULL
              ELSE convert_from(r.response_body, 'UTF8')::jsonb->'usage' END AS u
  FROM requests.requests q
  JOIN requests.turn_origins o ON o.id = q.turn_origin_id
  LEFT JOIN responses.responses r ON r.request_id = q.id
  WHERE q.created_at >= $1 AND q.session_id = ANY($2::bigint[])
    AND q.path = '/v1/messages' AND (r.id IS NULL OR r.response_status = 200)
)
SELECT session_id, agent_instance_id, claude_code_agent_id, origin,
       coalesce(sum(CASE WHEN u->>'input_tokens' ~ '^[0-9]+$'
                         THEN (u->>'input_tokens')::bigint END), 0)::bigint AS fresh,
       coalesce(sum(CASE WHEN u->>'cache_creation_input_tokens' ~ '^[0-9]+$'
                         THEN (u->>'cache_creation_input_tokens')::bigint END), 0)::bigint AS cache_write,
       coalesce(sum(CASE WHEN u->>'cache_read_input_tokens' ~ '^[0-9]+$'
                         THEN (u->>'cache_read_input_tokens')::bigint END), 0)::bigint AS cache_read,
       coalesce(sum(CASE WHEN u->>'output_tokens' ~ '^[0-9]+$'
                         THEN (u->>'output_tokens')::bigint END), 0)::bigint AS output,
       bool_and(u->>'input_tokens' ~ '^[0-9]+$' AND u->>'output_tokens' ~ '^[0-9]+$') AS complete_usage
FROM billed
GROUP BY session_id, agent_instance_id, claude_code_agent_id, origin";

const TURNS_SQL: &str = "
SELECT session_id, count(*)::bigint AS n
FROM memory.operator_turns
WHERE created_at >= $1 AND session_id = ANY($2::bigint[])
GROUP BY session_id";

const FILE_CALLS_SQL: &str = "
SELECT tu.id, tu.session_id, tu.created_at, tu.tool_name, tu.file_path, tu.input,
       q.agent_instance_id, q.claude_code_agent_id, o.code AS origin
FROM tools.uses tu
JOIN requests.requests q ON q.id = tu.request_id
JOIN requests.turn_origins o ON o.id = q.turn_origin_id
WHERE tu.created_at >= $1 AND tu.session_id = ANY($2::bigint[])
  AND tu.tool_name IN ('Read', 'Edit', 'MultiEdit', 'Write') AND tu.file_path IS NOT NULL";

const FILE_SOURCES_SQL: &str = "
SELECT tu.id, tu.wire_tool_use_id AS wire_id, tu.session_id, tu.file_path,
       NULL::jsonb AS input,
       q.agent_instance_id, q.claude_code_agent_id, o.code AS origin, tr.content,
       receipt.created_at AS result_at
FROM tools.uses tu
JOIN requests.requests q ON q.id = tu.request_id
JOIN requests.turn_origins o ON o.id = q.turn_origin_id
JOIN tools.results tr ON tr.tool_use_id = tu.id
JOIN requests.requests receipt ON receipt.id = tr.request_id
WHERE tu.wire_tool_use_id = ANY($1::text[]) AND NOT tr.is_error";

const SHELL_SOURCES_SQL: &str = "
SELECT tu.id, tu.wire_tool_use_id AS wire_id, tu.session_id, NULL::text AS file_path,
       tu.input,
       q.agent_instance_id, q.claude_code_agent_id, o.code AS origin, tr.content,
       receipt.created_at AS result_at
FROM tools.uses tu
JOIN requests.requests q ON q.id = tu.request_id
JOIN requests.turn_origins o ON o.id = q.turn_origin_id
JOIN tools.results tr ON tr.tool_use_id = tu.id
JOIN requests.requests receipt ON receipt.id = tr.request_id
WHERE tu.created_at >= $1 AND tu.session_id = ANY($2::bigint[]) AND tu.tool_name = 'Bash'
  AND tu.wire_tool_use_id = ANY($3::text[]) AND NOT tr.is_error";

const SHELL_CALLS_SQL: &str = "
SELECT tu.id, tu.session_id, tu.created_at, tu.tool_name, tu.file_path, tu.input,
       q.agent_instance_id, q.claude_code_agent_id, o.code AS origin
FROM tools.uses tu
JOIN requests.requests q ON q.id = tu.request_id
JOIN requests.turn_origins o ON o.id = q.turn_origin_id
WHERE tu.created_at >= $1 AND tu.session_id = ANY($2::bigint[])";

#[derive(Debug, Clone, sqlx::FromRow)]
pub(super) struct ArmRow {
    pub(super) session_id: i64,
    pub(super) arm: Option<String>,
    pub(super) observed_arms: i32,
}

pub(super) async fn begin_read_only(pool: &PgPool) -> Result<Transaction<'static, Postgres>> {
    let mut tx = pool
        .begin()
        .await
        .context("could not open a read-only transaction on the capture database")?;
    sqlx::query("SET TRANSACTION READ ONLY")
        .execute(&mut *tx)
        .await
        .context("could not make the transaction read-only")?;
    sqlx::query("SELECT set_config('statement_timeout', $1, true)")
        .bind(STATEMENT_TIMEOUT_MS)
        .execute(&mut *tx)
        .await
        .context("could not set the 300 s statement timeout")?;
    Ok(tx)
}

pub(super) fn fold_arm_chunks(chunks: Vec<Vec<ArmRow>>) -> (Arms, u64) {
    let mut observed: BTreeMap<i64, BTreeSet<Option<String>>> = BTreeMap::new();
    let mut mixed: BTreeSet<i64> = BTreeSet::new();
    for row in chunks.into_iter().flatten() {
        observed.entry(row.session_id).or_default().insert(row.arm);
        if row.observed_arms != 1 {
            mixed.insert(row.session_id);
        }
    }
    mixed.extend(
        observed
            .iter()
            .filter(|(_, arms)| arms.len() != 1)
            .map(|(session, _)| *session),
    );
    let arms = observed
        .into_iter()
        .filter(|(session, _)| !mixed.contains(session))
        .filter_map(|(session, mut arms)| Some((session, arms.pop_first()??)))
        .collect();
    (arms, mixed.len() as u64)
}

pub(super) async fn load_arms(
    pool: &PgPool,
    start: DateTime<Utc>,
    until: DateTime<Utc>,
    stage: &str,
) -> Result<(Arms, u64)> {
    let mut cursor = start;
    let mut chunks = Vec::new();
    while cursor < until {
        let end = (cursor + Duration::days(1)).min(until);
        let mut tx = begin_read_only(pool).await?;
        let rows = sqlx::query_as::<_, ArmRow>(ARMS_SQL)
            .bind(cursor)
            .bind(end)
            .bind(stage)
            .fetch_all(&mut *tx)
            .await
            .with_context(|| format!("arm query failed for {cursor} to {end}"))?;
        chunks.push(rows);
        cursor = end;
    }
    Ok(fold_arm_chunks(chunks))
}

pub(super) async fn load_cuts(
    pool: &PgPool,
    start: DateTime<Utc>,
    ids: &[i64],
    stage: &str,
    reasons: &[&str],
) -> Result<Vec<Cut>> {
    let reasons: Vec<String> = reasons.iter().map(|reason| reason.to_string()).collect();
    let mut tx = begin_read_only(pool).await?;
    sqlx::query_as::<_, Cut>(CUTS_SQL)
        .bind(start)
        .bind(ids)
        .bind(stage)
        .bind(reasons)
        .fetch_all(&mut *tx)
        .await
        .with_context(|| format!("cut query failed for stage {stage}"))
}

pub(super) type UsageTables = (
    BTreeMap<i64, Usage>,
    BTreeMap<Lane, Usage>,
    BTreeMap<i64, i64>,
);

pub(super) async fn load_usage(
    pool: &PgPool,
    start: DateTime<Utc>,
    ids: &[i64],
) -> Result<UsageTables> {
    let mut tx = begin_read_only(pool).await?;
    let rows = sqlx::query_as::<_, UsageRow>(USAGE_SQL)
        .bind(start)
        .bind(ids)
        .fetch_all(&mut *tx)
        .await
        .context("usage query failed")?;
    let turns = sqlx::query_as::<_, (i64, i64)>(TURNS_SQL)
        .bind(start)
        .bind(ids)
        .fetch_all(&mut *tx)
        .await
        .context("operator turn query failed")?;
    let (cost, lanes) = roll_up(&rows);
    Ok((cost, lanes, turns.into_iter().collect()))
}

pub(super) async fn load_file_calls(
    pool: &PgPool,
    start: DateTime<Utc>,
    ids: &[i64],
) -> Result<Vec<Call>> {
    let mut tx = begin_read_only(pool).await?;
    sqlx::query_as::<_, Call>(FILE_CALLS_SQL)
        .bind(start)
        .bind(ids)
        .fetch_all(&mut *tx)
        .await
        .context("file tool call query failed")
}

pub(super) async fn load_file_sources(pool: &PgPool, wanted: &[String]) -> Result<Vec<Source>> {
    let mut rows = Vec::new();
    for batch in wanted.chunks(SOURCE_BATCH) {
        let mut tx = begin_read_only(pool).await?;
        rows.extend(
            sqlx::query_as::<_, Source>(FILE_SOURCES_SQL)
                .bind(batch)
                .fetch_all(&mut *tx)
                .await
                .context("file read source query failed")?,
        );
    }
    Ok(rows)
}

pub(super) async fn load_shell_sources(
    pool: &PgPool,
    start: DateTime<Utc>,
    ids: &[i64],
    wanted: &[String],
) -> Result<Vec<Source>> {
    let mut rows = Vec::new();
    for batch in wanted.chunks(SOURCE_BATCH) {
        let mut tx = begin_read_only(pool).await?;
        rows.extend(
            sqlx::query_as::<_, Source>(SHELL_SOURCES_SQL)
                .bind(start)
                .bind(ids)
                .bind(batch)
                .fetch_all(&mut *tx)
                .await
                .context("shell output source query failed")?,
        );
    }
    Ok(rows)
}

pub(super) async fn load_shell_calls(
    pool: &PgPool,
    first: DateTime<Utc>,
    ids: &[i64],
) -> Result<Vec<Call>> {
    let mut tx = begin_read_only(pool).await?;
    sqlx::query_as::<_, Call>(SHELL_CALLS_SQL)
        .bind(first)
        .bind(ids)
        .fetch_all(&mut *tx)
        .await
        .context("shell tool call query failed")
}
