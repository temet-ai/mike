use std::collections::HashMap;

use chrono::{DateTime, Utc};
use sqlx::PgPool;

use crate::store::persistence::PersistError;

#[derive(Debug, sqlx::FromRow)]
pub struct SessionRequestIdRow {
    pub request_id: i64,
}

pub async fn list_session_request_ids_asc(
    pool: &PgPool,
    session_db_id: i64,
) -> Result<Vec<SessionRequestIdRow>, PersistError> {
    let rows = sqlx::query_as!(
        SessionRequestIdRow,
        r#"SELECT r.id AS "request_id!"
           FROM requests.requests r
           WHERE r.session_id = $1
           ORDER BY r.created_at ASC, r.id ASC"#,
        session_db_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BodySource {
    Request,
    RequestSent,
    Response,
}

#[derive(Debug, sqlx::FromRow)]
pub struct FindTextCandidateRow {
    pub request_id: i64,
    pub session_id: String,
    pub created_at: DateTime<Utc>,
}

pub struct FindTextQuery {
    pub source: BodySource,
    pub from: DateTime<Utc>,
    pub to: DateTime<Utc>,
    pub candidate_cap: i64,
}

pub async fn find_text_candidates(
    pool: &PgPool,
    needle: &[u8],
    query: FindTextQuery,
) -> Result<Vec<FindTextCandidateRow>, PersistError> {
    let FindTextQuery {
        source,
        from,
        to,
        candidate_cap,
    } = query;
    match source {
        BodySource::Request => {
            latest_request_body_candidates(pool, needle, from, to, candidate_cap).await
        }
        BodySource::RequestSent => {
            latest_sent_body_candidates(pool, needle, from, to, candidate_cap).await
        }
        BodySource::Response => {
            response_body_candidates(pool, needle, from, to, candidate_cap).await
        }
    }
}

async fn latest_request_body_candidates(
    pool: &PgPool,
    needle: &[u8],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    candidate_cap: i64,
) -> Result<Vec<FindTextCandidateRow>, PersistError> {
    let rows = sqlx::query_as!(
        FindTextCandidateRow,
        r#"SELECT request_id AS "request_id!",
                          session_id AS "session_id!",
                          created_at AS "created_at!"
                   FROM (
                       SELECT DISTINCT ON (r.session_id)
                            r.id                 AS request_id,
                            s.session_uuid::text AS session_id,
                            r.created_at,
                            r.request_body       AS body_col
                       FROM requests.requests  r
                       JOIN sessions.sessions  s ON s.id = r.session_id
                       WHERE r.created_at >= $1
                         AND r.created_at <  $2
                       ORDER BY r.session_id, r.created_at DESC, r.id DESC
                   ) latest
                   WHERE latest.body_col IS NOT NULL
                     AND position($3 IN latest.body_col) > 0
                   ORDER BY latest.created_at DESC
                   LIMIT $4"#,
        from,
        to,
        needle,
        candidate_cap,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

async fn latest_sent_body_candidates(
    pool: &PgPool,
    needle: &[u8],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    candidate_cap: i64,
) -> Result<Vec<FindTextCandidateRow>, PersistError> {
    let rows = sqlx::query_as!(
        FindTextCandidateRow,
        r#"SELECT request_id AS "request_id!",
                          session_id AS "session_id!",
                          created_at AS "created_at!"
                   FROM (
                       SELECT DISTINCT ON (r.session_id)
                            r.id                 AS request_id,
                            s.session_uuid::text AS session_id,
                            r.created_at,
                            COALESCE(r.filtered_request_body, r.request_body) AS body_col
                       FROM requests.requests  r
                       JOIN sessions.sessions  s ON s.id = r.session_id
                       WHERE r.created_at >= $1
                         AND r.created_at <  $2
                       ORDER BY r.session_id, r.created_at DESC, r.id DESC
                   ) latest
                   WHERE latest.body_col IS NOT NULL
                     AND position($3 IN latest.body_col) > 0
                   ORDER BY latest.created_at DESC
                   LIMIT $4"#,
        from,
        to,
        needle,
        candidate_cap,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

async fn response_body_candidates(
    pool: &PgPool,
    needle: &[u8],
    from: DateTime<Utc>,
    to: DateTime<Utc>,
    candidate_cap: i64,
) -> Result<Vec<FindTextCandidateRow>, PersistError> {
    let rows = sqlx::query_as!(
        FindTextCandidateRow,
        r#"SELECT
                       r.id                 AS "request_id!",
                       s.session_uuid::text AS "session_id!",
                       r.created_at         AS "created_at!"
                   FROM responses.responses resp
                   JOIN requests.requests   r ON r.id  = resp.request_id
                   JOIN sessions.sessions   s ON s.id  = r.session_id
                   WHERE r.created_at   >= $1
                     AND r.created_at   <  $2
                     AND resp.response_body != '\x'::bytea
                     AND position($3 IN resp.response_body) > 0
                   ORDER BY r.created_at DESC, r.id DESC
                   LIMIT $4"#,
        from,
        to,
        needle,
        candidate_cap,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}

pub async fn fetch_response_bodies_batch(
    pool: &PgPool,
    request_ids: &[i64],
) -> Result<HashMap<i64, bytes::Bytes>, PersistError> {
    if request_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query!(
        r#"SELECT request_id AS "request_id!", response_body AS "response_body!"
           FROM responses.responses
           WHERE request_id = ANY($1)"#,
        request_ids,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.request_id, bytes::Bytes::from(r.response_body)))
        .collect())
}

pub async fn fetch_request_bodies_batch(
    pool: &PgPool,
    request_ids: &[i64],
) -> Result<HashMap<i64, bytes::Bytes>, PersistError> {
    if request_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query!(
        r#"SELECT id AS "id!", request_body AS "request_body!"
           FROM requests.requests
           WHERE id = ANY($1)
             AND request_body IS NOT NULL"#,
        request_ids,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .map(|r| (r.id, bytes::Bytes::from(r.request_body)))
        .collect())
}
