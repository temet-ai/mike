use sqlx::{PgConnection, PgPool};

use super::assemble::{StoredRequest, assemble_sent};
use super::parts::{envelope_patch, load_parts};
use crate::store::persistence::PersistError;

pub(crate) async fn fetch_sent_body(
    pool: &PgPool,
    request_id: i64,
) -> Result<Option<bytes::Bytes>, PersistError> {
    let mut conn = pool.acquire().await?;
    let row = sqlx::query!(
        "SELECT filtered_request_body, request_envelope, sent_envelope, sent_message_count,
                sent_patch_start, sent_patch_end, sent_patch_bytes
           FROM requests.requests
          WHERE id = $1",
        request_id,
    )
    .fetch_optional(&mut *conn)
    .await?;
    let Some(row) = row else {
        return Ok(None);
    };
    if let Some(legacy) = row.filtered_request_body {
        return Ok(Some(bytes::Bytes::from(legacy)));
    }
    let request = StoredRequest {
        id: request_id,
        request_envelope: row.request_envelope,
        sent_envelope: row.sent_envelope,
        sent_message_count: row.sent_message_count,
        envelope_patch: envelope_patch(
            row.sent_patch_start,
            row.sent_patch_end,
            row.sent_patch_bytes,
        ),
    };
    let loaded = load_parts(&mut conn, &[request_id]).await?;
    let parts = loaded.for_request(request_id);
    let raw_body = if request.needs_raw_body(parts) {
        fetch_raw_body(&mut conn, request_id).await?
    } else {
        None
    };
    Ok(assemble_sent(&request, parts, raw_body.as_deref())?.map(bytes::Bytes::from))
}

async fn fetch_raw_body(
    conn: &mut PgConnection,
    request_id: i64,
) -> Result<Option<Vec<u8>>, PersistError> {
    let body = sqlx::query_scalar!(
        "SELECT request_body FROM requests.requests WHERE id = $1",
        request_id,
    )
    .fetch_optional(&mut *conn)
    .await?;
    Ok(body.flatten())
}
