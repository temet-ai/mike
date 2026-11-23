use std::collections::HashMap;

use serde_json::Value as JsonValue;
use sqlx::PgConnection;

use crate::store::persistence::PersistError;
use crate::store::persistence::messages::{CanonicalMessage, insert_new_message_bodies, invariant};

pub(crate) struct SentRow<'a> {
    pub request_id: i64,
    pub message: &'a CanonicalMessage,
}

pub(crate) async fn insert_sent_messages(
    conn: &mut PgConnection,
    rows: &[SentRow<'_>],
    known: &HashMap<String, i64>,
) -> Result<(), PersistError> {
    if rows.is_empty() {
        return Ok(());
    }
    let new_ids = insert_new_message_bodies(conn, rows.iter().map(|r| r.message), known).await?;
    let mut request_ids = Vec::with_capacity(rows.len());
    let mut ords = Vec::with_capacity(rows.len());
    let mut message_ids = Vec::with_capacity(rows.len());
    let mut cache_controls: Vec<Option<JsonValue>> = Vec::with_capacity(rows.len());
    for row in rows {
        let sha = &row.message.sha;
        let Some(message_id) = known.get(sha).or_else(|| new_ids.get(sha)).copied() else {
            return Err(invariant(format!(
                "request {} sent message {} sha {sha} resolved to no message_id",
                row.request_id, row.message.ord
            )));
        };
        request_ids.push(row.request_id);
        ords.push(row.message.ord);
        message_ids.push(message_id);
        cache_controls.push(row.message.cache_control.clone());
    }
    sqlx::query!(
        "INSERT INTO requests.sent_messages (request_id, ord, message_id, cache_control)
         SELECT u.request_id, u.ord, u.message_id, u.cache_control
           FROM UNNEST($1::bigint[], $2::int[], $3::bigint[], $4::jsonb[])
                AS u(request_id, ord, message_id, cache_control)
         ON CONFLICT (request_id, ord) DO NOTHING",
        &request_ids,
        &ords,
        &message_ids,
        &cache_controls as &[Option<JsonValue>],
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(crate) async fn set_sent_envelopes(
    conn: &mut PgConnection,
    ids: &[i64],
    envelopes: &[Vec<u8>],
) -> Result<(), PersistError> {
    if ids.is_empty() {
        return Ok(());
    }
    sqlx::query!(
        "UPDATE requests.requests r
            SET sent_envelope = v.envelope
           FROM UNNEST($1::bigint[], $2::bytea[]) AS v(id, envelope)
          WHERE r.id = v.id",
        ids,
        envelopes,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(crate) async fn set_sent_message_counts(
    conn: &mut PgConnection,
    ids: &[i64],
    counts: &[i32],
) -> Result<(), PersistError> {
    if ids.is_empty() {
        return Ok(());
    }
    sqlx::query!(
        "UPDATE requests.requests r
            SET sent_message_count = v.count
           FROM UNNEST($1::bigint[], $2::int[]) AS v(id, count)
          WHERE r.id = v.id",
        ids,
        counts,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(crate) async fn clear_sent_content(
    conn: &mut PgConnection,
    ids: &[i64],
) -> Result<(), PersistError> {
    if ids.is_empty() {
        return Ok(());
    }
    sqlx::query!(
        "DELETE FROM requests.sent_messages WHERE request_id = ANY($1)",
        ids
    )
    .execute(&mut *conn)
    .await?;
    sqlx::query!(
        "UPDATE requests.requests
            SET sent_envelope = NULL, sent_message_count = NULL
          WHERE id = ANY($1)",
        ids,
    )
    .execute(&mut *conn)
    .await?;
    Ok(())
}

pub(crate) async fn sent_message_ids(
    conn: &mut PgConnection,
    request_ids: &[i64],
) -> Result<HashMap<String, i64>, PersistError> {
    if request_ids.is_empty() {
        return Ok(HashMap::new());
    }
    let rows = sqlx::query!(
        "SELECT m.message_sha256, sm.message_id
           FROM requests.sent_messages sm
           JOIN messages.messages m ON m.id = sm.message_id
          WHERE sm.request_id = ANY($1)",
        request_ids,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| (row.message_sha256, row.message_id))
        .collect())
}

pub(crate) struct BridgedMessage {
    pub sha: String,
    pub message_id: i64,
    pub cache_control: Option<JsonValue>,
}

pub(crate) async fn bridged_messages(
    conn: &mut PgConnection,
    request_ids: &[i64],
) -> Result<HashMap<i64, Vec<BridgedMessage>>, PersistError> {
    let rows = sqlx::query!(
        "SELECT rm.request_id, m.message_sha256, rm.message_id, rm.cache_control
           FROM requests.messages rm
           JOIN messages.messages m ON m.id = rm.message_id
          WHERE rm.request_id = ANY($1)
          ORDER BY rm.request_id, rm.ord",
        request_ids,
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut out: HashMap<i64, Vec<BridgedMessage>> = HashMap::new();
    for row in rows {
        out.entry(row.request_id).or_default().push(BridgedMessage {
            sha: row.message_sha256,
            message_id: row.message_id,
            cache_control: row.cache_control,
        });
    }
    Ok(out)
}
