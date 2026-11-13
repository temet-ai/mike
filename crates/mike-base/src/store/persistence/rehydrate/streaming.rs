use sqlx::PgPool;

use crate::store::models::PersistedStreamBlockRow;
use crate::store::persistence::PersistError;

pub async fn fetch_accumulated_text_for_request(
    pool: &PgPool,
    request_id: i64,
) -> Result<Option<String>, PersistError> {
    let text = sqlx::query_scalar!(
        r#"SELECT string_agg(b.content, '' ORDER BY b.seq_ord) AS "text?"
             FROM responses.stream_blocks b
             JOIN responses.stream_block_types bt ON bt.id = b.block_type_id
            WHERE b.request_id = $1 AND bt.code = 'text'"#,
        request_id,
    )
    .fetch_one(pool)
    .await?;
    Ok(text)
}

pub async fn fetch_stream_blocks_for_request(
    pool: &PgPool,
    request_id: i64,
) -> Result<Vec<PersistedStreamBlockRow>, PersistError> {
    let rows = sqlx::query_as!(
        PersistedStreamBlockRow,
        "SELECT b.seq_ord, b.content, b.byte_size,
                b.content_sha256, bt.code AS block_type,
                b.signature, b.wire_tool_use_id, b.arrived_at
         FROM responses.stream_blocks b
         JOIN responses.stream_block_types bt ON bt.id = b.block_type_id
         WHERE b.request_id = $1
         ORDER BY b.seq_ord ASC",
        request_id,
    )
    .fetch_all(pool)
    .await?;
    Ok(rows)
}
