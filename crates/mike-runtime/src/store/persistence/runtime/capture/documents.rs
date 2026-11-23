use std::collections::HashMap;

use sqlx::{PgConnection, PgPool};

use super::{CaptureChunkInput, invalid};
use crate::store::persistence::PersistError;

#[path = "append.rs"]
mod append;

pub(super) async fn create(pool: &PgPool, predecessor: Option<i64>) -> Result<i64, PersistError> {
    Ok(sqlx::query_scalar!(
        "INSERT INTO runtime.capture_documents (predecessor_id) VALUES ($1) RETURNING id",
        predecessor
    )
    .fetch_one(pool)
    .await?)
}

pub(super) async fn append(
    pool: &PgPool,
    document: i64,
    start: i64,
    chunks: &[CaptureChunkInput],
) -> Result<Vec<i64>, PersistError> {
    let end = start
        .checked_add(i64::try_from(chunks.len()).map_err(|_| invalid("too many chunk ordinals"))?)
        .filter(|_| start >= 0)
        .ok_or_else(|| invalid("invalid chunk ordinal"))?;
    let mut tx = pool.begin().await?;
    let row = sqlx::query!(
        "SELECT predecessor_id, chunk_count, finished FROM runtime.capture_documents WHERE id = $1 FOR UPDATE",
        document
    ).fetch_one(&mut *tx).await?;
    if start < row.chunk_count && end <= row.chunk_count {
        let ids = replay(&mut tx, document, start, end, chunks).await?;
        tx.commit().await?;
        return Ok(ids);
    }
    if start != row.chunk_count || row.finished {
        return Err(invalid(
            "capture chunks must append contiguously to an unfinished document",
        ));
    }
    append::validate_references(&mut tx, row.predecessor_id, chunks).await?;
    let ids = append::insert_chunks(&mut tx, document, start, chunks).await?;
    append::insert_references(&mut tx, document, start, &ids).await?;
    tx.commit().await?;
    Ok(ids)
}

async fn replay(
    connection: &mut PgConnection,
    document: i64,
    start: i64,
    end: i64,
    chunks: &[CaptureChunkInput],
) -> Result<Vec<i64>, PersistError> {
    let existing = sqlx::query!(
        "SELECT r.ordinal, c.id, c.body FROM runtime.capture_document_chunks r
         JOIN runtime.capture_chunks c ON c.id = r.chunk_id
         WHERE r.document_id = $1 AND r.ordinal >= $2 AND r.ordinal < $3 ORDER BY r.ordinal",
        document,
        start,
        end
    )
    .fetch_all(connection)
    .await?;
    if existing.len() != chunks.len()
        || existing.iter().zip(chunks).any(|(row, input)| match input {
            CaptureChunkInput::New(body) => &row.body != body,
            CaptureChunkInput::Existing(id) => row.id != *id,
        })
    {
        return Err(invalid(
            "capture chunk replay changed already captured bytes",
        ));
    }
    Ok(existing.into_iter().map(|row| row.id).collect())
}

pub(super) async fn finish(
    pool: &PgPool,
    document: i64,
    complete: bool,
) -> Result<(), PersistError> {
    let result = sqlx::query!(
        "UPDATE runtime.capture_documents SET finished = true, complete = $2
         WHERE id = $1 AND (NOT finished OR complete = $2)",
        document,
        complete
    )
    .execute(pool)
    .await?;
    if result.rows_affected() != 1 {
        return Err(invalid(
            "missing capture document or conflicting completion state",
        ));
    }
    Ok(())
}
