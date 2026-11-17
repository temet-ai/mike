use chrono::{DateTime, Utc};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgPool;

#[derive(Debug, Clone, Serialize)]
pub(crate) struct CauseGroup {
    pub(crate) kind: String,
    pub(crate) cause: String,
    pub(crate) stage: Option<String>,
    pub(crate) location: Option<String>,
    pub(crate) woken_by: Option<String>,
    pub(crate) breaks: i64,
    pub(crate) sessions: i64,
    pub(crate) tokens_rewritten: i64,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct BreakRow {
    pub(crate) id: i64,
    pub(crate) request_id: i64,
    pub(crate) session_id: String,
    pub(crate) session_title: Option<String>,
    pub(crate) prev_request_id: Option<i64>,
    pub(crate) source_request_id: Option<i64>,
    pub(crate) kind: String,
    pub(crate) cause: String,
    pub(crate) stage: Option<String>,
    pub(crate) location: Option<String>,
    pub(crate) woken_by: Option<String>,
    pub(crate) ttl_seconds: i32,
    pub(crate) gap_seconds: i32,
    pub(crate) expected_read: i64,
    pub(crate) cache_read: i64,
    pub(crate) tokens_rewritten: i64,
    pub(crate) detail: Value,
    pub(crate) occurred_at: DateTime<Utc>,
}

pub(crate) async fn groups(
    pool: &PgPool,
    session_id: Option<i64>,
    since: DateTime<Utc>,
) -> Result<Vec<CauseGroup>, sqlx::Error> {
    sqlx::query_as!(
        CauseGroup,
        r#"SELECT b.kind AS "kind!", b.cause AS "cause!", b.stage, b.location,
                  CASE WHEN b.kind = 'expiry' THEN b.woken_by END AS woken_by,
                  count(*) AS "breaks!", count(DISTINCT b.session_id) AS "sessions!",
                  sum(b.tokens_rewritten)::bigint AS "tokens_rewritten!"
             FROM prompt_cache.breaks b
            WHERE b.occurred_at >= $2
              AND ($1::bigint IS NULL OR b.session_id = $1)
            GROUP BY 1, 2, 3, 4, 5
            ORDER BY 8 DESC"#,
        session_id,
        since,
    )
    .fetch_all(pool)
    .await
}

pub(crate) async fn rows(
    pool: &PgPool,
    session_id: Option<i64>,
    since: DateTime<Utc>,
    limit: i64,
    offset: i64,
) -> Result<Vec<BreakRow>, sqlx::Error> {
    sqlx::query_as!(
        BreakRow,
        r#"SELECT b.id, b.request_id, s.session_uuid::text AS "session_id!", s.title AS session_title,
                  b.prev_request_id, b.source_request_id, b.kind, b.cause, b.stage, b.location,
                  b.woken_by, b.ttl_seconds, b.gap_seconds, b.expected_read, b.cache_read,
                  b.tokens_rewritten, b.detail, b.occurred_at
             FROM prompt_cache.breaks b
             JOIN sessions.sessions s ON s.id = b.session_id
            WHERE b.occurred_at >= $2
              AND ($1::bigint IS NULL OR b.session_id = $1)
            ORDER BY b.occurred_at DESC, b.id DESC
            LIMIT $3 OFFSET $4"#,
        session_id,
        since,
        limit,
        offset,
    )
    .fetch_all(pool)
    .await
}
