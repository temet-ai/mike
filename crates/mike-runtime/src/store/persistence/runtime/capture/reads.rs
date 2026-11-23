use sqlx::PgPool;
use uuid::Uuid;

use super::{CaptureDocumentRow, CaptureExchangeRow, KnownCaptureChunk};
use crate::store::persistence::PersistError;

pub(super) async fn document(
    pool: &PgPool,
    document: i64,
) -> Result<Option<CaptureDocumentRow>, PersistError> {
    Ok(sqlx::query_as!(CaptureDocumentRow,
        "SELECT id, byte_size, chunk_count, finished, complete FROM runtime.capture_documents WHERE id = $1",
        document
    ).fetch_optional(pool).await?)
}

pub(super) async fn chunks(
    pool: &PgPool,
    document: i64,
    offset: i64,
    limit: i64,
) -> Result<Vec<KnownCaptureChunk>, PersistError> {
    Ok(sqlx::query_as!(
        KnownCaptureChunk,
        "SELECT c.id, r.ordinal, c.body FROM runtime.capture_document_chunks r
         JOIN runtime.capture_chunks c ON c.id = r.chunk_id
         WHERE r.document_id = $1 AND r.ordinal >= $2 ORDER BY r.ordinal LIMIT $3",
        document,
        offset.max(0),
        limit.clamp(1, 256)
    )
    .fetch_all(pool)
    .await?)
}

pub(super) async fn exchange(
    pool: &PgPool,
    id: Uuid,
) -> Result<Option<CaptureExchangeRow>, PersistError> {
    Ok(sqlx::query_as!(CaptureExchangeRow,
        "SELECT e.exchange_uuid AS id, s.session_uuid, e.provider_thread_id, e.created_at, e.metadata,
                e.request_document_id, e.sent_document_id, e.response_document_id, e.status
         FROM runtime.capture_exchanges e LEFT JOIN sessions.sessions s ON s.id = e.session_id
         WHERE e.exchange_uuid = $1",
        id
    ).fetch_optional(pool).await?)
}

pub(super) async fn exchanges(
    pool: &PgPool,
    session: Uuid,
    limit: i64,
    offset: i64,
) -> Result<Vec<CaptureExchangeRow>, PersistError> {
    Ok(sqlx::query_as!(CaptureExchangeRow,
        r#"SELECT e.exchange_uuid AS id, s.session_uuid AS "session_uuid?", e.provider_thread_id, e.created_at, e.metadata,
                e.request_document_id, e.sent_document_id, e.response_document_id, e.status
         FROM runtime.capture_exchanges e JOIN sessions.sessions s ON s.id = e.session_id
         WHERE s.session_uuid = $1 ORDER BY e.created_at DESC, e.id DESC LIMIT $2 OFFSET $3"#,
        session, limit.clamp(1, 200), offset.max(0)
    ).fetch_all(pool).await?)
}

pub(super) async fn previous(
    pool: &PgPool,
    session: Option<Uuid>,
    thread: Option<&str>,
) -> Result<Option<i64>, PersistError> {
    if let Some(session) = session {
        return Ok(sqlx::query_scalar!(
            "SELECT e.request_document_id FROM runtime.capture_exchanges e
             JOIN sessions.sessions s ON s.id = e.session_id
             JOIN runtime.capture_documents d ON d.id = e.request_document_id
             WHERE s.session_uuid = $1 AND ($2::text IS NULL OR e.provider_thread_id = $2)
               AND d.finished AND d.complete AND d.byte_size > 0
             ORDER BY e.created_at DESC, e.id DESC LIMIT 1",
            session,
            thread
        )
        .fetch_optional(pool)
        .await?);
    }
    let Some(thread) = thread else {
        return Ok(None);
    };
    Ok(sqlx::query_scalar!(
        "SELECT e.request_document_id FROM runtime.capture_exchanges e
         JOIN runtime.capture_documents d ON d.id = e.request_document_id
         WHERE e.provider_thread_id = $1 AND d.finished AND d.complete AND d.byte_size > 0
         ORDER BY e.created_at DESC, e.id DESC LIMIT 1",
        thread
    )
    .fetch_optional(pool)
    .await?)
}

pub(super) async fn previous_response(
    pool: &PgPool,
    session: Option<Uuid>,
    thread: Option<&str>,
) -> Result<Option<i64>, PersistError> {
    if let Some(session) = session {
        return Ok(sqlx::query_scalar!(
            "SELECT d.id FROM runtime.capture_exchanges e
             JOIN sessions.sessions s ON s.id = e.session_id
             JOIN runtime.capture_documents d ON d.id = e.response_document_id
             WHERE s.session_uuid = $1 AND ($2::text IS NULL OR e.provider_thread_id = $2)
               AND d.finished AND d.byte_size > 0
             ORDER BY e.created_at DESC, e.id DESC LIMIT 1",
            session,
            thread
        )
        .fetch_optional(pool)
        .await?);
    }
    let Some(thread) = thread else {
        return Ok(None);
    };
    Ok(sqlx::query_scalar!(
        "SELECT d.id FROM runtime.capture_exchanges e
         JOIN runtime.capture_documents d ON d.id = e.response_document_id
         WHERE e.provider_thread_id = $1 AND d.finished AND d.byte_size > 0
         ORDER BY e.created_at DESC, e.id DESC LIMIT 1",
        thread
    )
    .fetch_optional(pool)
    .await?)
}
