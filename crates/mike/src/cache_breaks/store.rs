use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::PgPool;

use super::detect::{Candidate, MIN_REWRITE_TOKENS, Usage};

pub(crate) struct Found {
    pub(crate) request_id: i64,
    pub(crate) session_id: i64,
    pub(crate) prev_request_id: i64,
    pub(crate) source_request_id: Option<i64>,
    pub(crate) kind: &'static str,
    pub(crate) cause: &'static str,
    pub(crate) stage: Option<String>,
    pub(crate) location: Option<&'static str>,
    pub(crate) woken_by: Option<String>,
    pub(crate) ttl_seconds: i64,
    pub(crate) gap_seconds: i64,
    pub(crate) expected_read: i64,
    pub(crate) cache_read: i64,
    pub(crate) tokens_rewritten: i64,
    pub(crate) detail: Value,
    pub(crate) occurred_at: DateTime<Utc>,
}

pub(crate) async fn cursor(pool: &PgPool) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar!("SELECT last_response_id FROM prompt_cache.scan_state WHERE id = 1")
        .fetch_optional(pool)
        .await
}

pub(crate) async fn first_response_since(
    pool: &PgPool,
    since: DateTime<Utc>,
) -> Result<i64, sqlx::Error> {
    let first = sqlx::query_scalar!(
        "SELECT min(id) FROM responses.responses WHERE completed_at >= $1",
        since,
    )
    .fetch_one(pool)
    .await?;
    Ok(first.map_or(0, |id| id - 1))
}

pub(crate) async fn settled_upper(pool: &PgPool) -> Result<i64, sqlx::Error> {
    let upper = sqlx::query_scalar!(
        "SELECT id FROM responses.responses
          WHERE persisted_at < now() - interval '1 minute'
          ORDER BY id DESC LIMIT 1"
    )
    .fetch_optional(pool)
    .await?;
    Ok(upper.unwrap_or(0))
}

pub(crate) async fn set_cursor(pool: &PgPool, last_response_id: i64) -> Result<(), sqlx::Error> {
    sqlx::query!(
        "INSERT INTO prompt_cache.scan_state (id, last_response_id) VALUES (1, $1)
         ON CONFLICT (id) DO UPDATE SET last_response_id = EXCLUDED.last_response_id, updated_at = now()",
        last_response_id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

pub(crate) async fn candidates(
    pool: &PgPool,
    after: i64,
    upto: i64,
) -> Result<Vec<Candidate>, sqlx::Error> {
    let rows = sqlx::query_file!(
        "src/cache_breaks/candidates.sql",
        after,
        upto,
        MIN_REWRITE_TOKENS
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| Candidate {
            request_id: r.request_id,
            session_id: r.session_id,
            model: r.model,
            created_at: r.created_at,
            woken_by: r.woken_by,
            usage: Usage {
                cache_read: r.cache_read,
                cache_creation: r.cache_creation,
                input: r.input,
                creation_1h: r.creation_1h,
                creation_5m: r.creation_5m,
            },
            prev_request_id: r.prev_request_id,
            prev_model: r.prev_model,
            prev_done_at: r.prev_done_at,
            prev_usage: Usage {
                cache_read: r.prev_cache_read,
                cache_creation: r.prev_cache_creation,
                input: 0,
                creation_1h: r.prev_creation_1h,
                creation_5m: r.prev_creation_5m,
            },
        })
        .collect())
}

pub(crate) async fn manifests(
    pool: &PgPool,
    ids: &[i64],
) -> Result<Vec<(i64, Option<Value>)>, sqlx::Error> {
    let rows = sqlx::query!(
        "SELECT id, transformation_manifest FROM requests.requests WHERE id = ANY($1)",
        ids,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.id, r.transformation_manifest))
        .collect())
}

pub(crate) async fn linked_request(
    pool: &PgPool,
    session_id: i64,
    message_id: &str,
) -> Result<Option<i64>, sqlx::Error> {
    sqlx::query_scalar!(
        "SELECT r.id FROM requests.requests r
           JOIN responses.responses rs ON rs.request_id = r.id
          WHERE r.session_id = $1 AND rs.upstream_message_id = $2
          ORDER BY r.id DESC LIMIT 1",
        session_id,
        message_id,
    )
    .fetch_optional(pool)
    .await
}

pub(crate) async fn insert(pool: &PgPool, found: &[Found]) -> Result<u64, sqlx::Error> {
    if found.is_empty() {
        return Ok(0);
    }
    let col = |f: fn(&Found) -> i64| found.iter().map(f).collect::<Vec<i64>>();
    let opt =
        |f: fn(&Found) -> Option<String>| found.iter().map(f).collect::<Vec<Option<String>>>();
    let text = |f: fn(&Found) -> &'static str| {
        found
            .iter()
            .map(|x| f(x).to_owned())
            .collect::<Vec<String>>()
    };
    let source: Vec<Option<i64>> = found.iter().map(|f| f.source_request_id).collect();
    let detail: Vec<Value> = found.iter().map(|f| f.detail.clone()).collect();
    let occurred: Vec<DateTime<Utc>> = found.iter().map(|f| f.occurred_at).collect();
    let done = sqlx::query_file!(
        "src/cache_breaks/insert.sql",
        &col(|f| f.request_id),
        &col(|f| f.session_id),
        &col(|f| f.prev_request_id),
        &source as &[Option<i64>],
        &text(|f| f.kind),
        &text(|f| f.cause),
        &opt(|f| f.stage.clone()) as &[Option<String>],
        &opt(|f| f.location.map(str::to_owned)) as &[Option<String>],
        &opt(|f| f.woken_by.clone()) as &[Option<String>],
        &col(|f| f.ttl_seconds),
        &col(|f| f.gap_seconds),
        &col(|f| f.expected_read),
        &col(|f| f.cache_read),
        &col(|f| f.tokens_rewritten),
        &detail,
        &occurred,
    )
    .execute(pool)
    .await?;
    Ok(done.rows_affected())
}
