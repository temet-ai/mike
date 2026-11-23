use sqlx::{PgConnection, PgPool};
use uuid::Uuid;

use super::{CaptureExchangeInput, invalid};
use crate::store::persistence::PersistError;

pub(super) async fn upsert(
    pool: &PgPool,
    input: &CaptureExchangeInput,
) -> Result<(), PersistError> {
    validate_input(input)?;
    let mut tx = pool.begin().await?;
    let session = resolve_session(&mut tx, input.session_uuid).await?;
    validate_documents(&mut tx, input, session).await?;
    let (session, changed) = write_exchange(&mut tx, input, session).await?;
    advance_activity(&mut tx, session, changed).await?;
    tx.commit().await?;
    Ok(())
}

fn validate_input(input: &CaptureExchangeInput) -> Result<(), PersistError> {
    if !input.metadata.is_object()
        || !matches!(
            input.status.as_str(),
            "pending" | "complete" | "error" | "aborted" | "refused"
        )
    {
        return Err(invalid("invalid capture exchange metadata or status"));
    }
    Ok(())
}

async fn resolve_session(
    connection: &mut PgConnection,
    session: Option<Uuid>,
) -> Result<Option<i64>, PersistError> {
    Ok(match session {
        Some(uuid) => Some(
            sqlx::query_scalar!(
                "SELECT id FROM sessions.sessions WHERE session_uuid = $1 FOR KEY SHARE",
                uuid
            )
            .fetch_one(connection)
            .await?,
        ),
        None => None,
    })
}

async fn write_exchange(
    connection: &mut PgConnection,
    input: &CaptureExchangeInput,
    session: Option<i64>,
) -> Result<(Option<i64>, bool), PersistError> {
    let result = sqlx::query!(
        r#"INSERT INTO runtime.capture_exchanges AS e
            (exchange_uuid, session_id, provider_thread_id, created_at, metadata,
             request_document_id, sent_document_id, response_document_id, status)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (exchange_uuid) DO UPDATE SET
             session_id = COALESCE(EXCLUDED.session_id, e.session_id),
             provider_thread_id = COALESCE(EXCLUDED.provider_thread_id, e.provider_thread_id),
             metadata = EXCLUDED.metadata,
             response_document_id = COALESCE(EXCLUDED.response_document_id, e.response_document_id),
             status = EXCLUDED.status
         WHERE e.request_document_id = EXCLUDED.request_document_id
           AND e.sent_document_id = EXCLUDED.sent_document_id
           AND e.created_at = EXCLUDED.created_at
           AND (e.session_id IS NULL OR EXCLUDED.session_id IS NULL OR e.session_id = EXCLUDED.session_id)
           AND (e.provider_thread_id IS NULL OR EXCLUDED.provider_thread_id IS NULL OR e.provider_thread_id = EXCLUDED.provider_thread_id)
           AND (e.response_document_id IS NULL OR EXCLUDED.response_document_id IS NULL OR e.response_document_id = EXCLUDED.response_document_id)
           AND (e.status = 'pending' OR e.status = EXCLUDED.status)
         RETURNING WITH (OLD AS before, NEW AS after) after.session_id,
             (before.id IS NULL OR
              ROW(before.session_id, before.provider_thread_id, before.metadata, before.response_document_id, before.status)
              IS DISTINCT FROM
              ROW(after.session_id, after.provider_thread_id, after.metadata, after.response_document_id, after.status)) AS "changed!""#,
        input.id, session, input.provider_thread_id, input.created_at, input.metadata,
        input.request_document_id, input.sent_document_id, input.response_document_id, input.status
    ).fetch_optional(connection).await?
        .ok_or_else(|| invalid(
            "capture exchange identity reused with different documents or ownership",
        ))?;
    Ok((result.session_id, result.changed))
}

async fn advance_activity(
    connection: &mut PgConnection,
    session: Option<i64>,
    changed: bool,
) -> Result<(), PersistError> {
    if changed {
        sqlx::query!(
            "UPDATE sessions.sessions SET last_seen = GREATEST(last_seen + interval '1 microsecond', clock_timestamp()) WHERE id = $1",
            session
        ).execute(connection).await?;
    }
    Ok(())
}

async fn validate_documents(
    connection: &mut PgConnection,
    input: &CaptureExchangeInput,
    session: Option<i64>,
) -> Result<(), PersistError> {
    let mut ids = vec![input.request_document_id, input.sent_document_id];
    ids.extend(input.response_document_id);
    ids.sort_unstable();
    ids.dedup();
    sqlx::query!(
        "SELECT id FROM runtime.capture_documents WHERE id = ANY($1) ORDER BY id FOR UPDATE",
        &ids
    )
    .fetch_all(&mut *connection)
    .await?;
    let valid = sqlx::query_scalar!(
        r#"SELECT NOT EXISTS (
            SELECT 1 FROM UNNEST($1::bigint[]) AS wanted(id)
            LEFT JOIN runtime.capture_documents d ON d.id = wanted.id
            WHERE d.id IS NULL
               OR EXISTS (SELECT 1 FROM runtime.capture_exchanges e
                          WHERE e.exchange_uuid <> $2 AND
                            (e.request_document_id = d.id OR e.sent_document_id = d.id OR e.response_document_id = d.id))
               OR (d.predecessor_id IS NOT NULL AND NOT d.predecessor_id = ANY($1)
                   AND NOT EXISTS (
                       SELECT 1 FROM runtime.capture_exchanges p
                       WHERE (p.request_document_id = d.predecessor_id OR p.sent_document_id = d.predecessor_id OR p.response_document_id = d.predecessor_id)
                         AND (($3::bigint IS NOT NULL AND p.session_id = $3)
                              OR ($4::text IS NOT NULL AND p.provider_thread_id = $4
                                  AND ($3::bigint IS NULL OR p.session_id IS NULL OR p.session_id = $3)))
                   ))
        ) AS "valid!""#,
        &ids, input.id, session, input.provider_thread_id
    ).fetch_one(connection).await?;
    if !valid {
        return Err(invalid(
            "capture document is missing, already owned, or has a predecessor from another session",
        ));
    }
    Ok(())
}
