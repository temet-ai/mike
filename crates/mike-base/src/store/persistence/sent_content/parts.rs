use std::collections::HashMap;

use serde_json::Value as JsonValue;
use sqlx::PgConnection;

use super::assemble::{RequestParts, StoredRequest};
use crate::store::persistence::PersistError;
use crate::store::persistence::messages::SentPatchRow;
use crate::store::persistence::reconstruct::restore_cache_control;

#[derive(Debug, Default)]
pub(crate) struct LoadedParts {
    original_messages: HashMap<i64, Vec<JsonValue>>,
    tools: HashMap<i64, Vec<u8>>,
    sent_messages: HashMap<i64, Vec<(i32, JsonValue)>>,
    delta_patches: HashMap<i64, Vec<SentPatchRow>>,
}

impl LoadedParts {
    pub fn for_request(&self, request_id: i64) -> RequestParts<'_> {
        RequestParts {
            original_messages: self
                .original_messages
                .get(&request_id)
                .map(Vec::as_slice)
                .unwrap_or_default(),
            tools: self
                .tools
                .get(&request_id)
                .map(Vec::as_slice)
                .unwrap_or_default(),
            sent_messages: self
                .sent_messages
                .get(&request_id)
                .map(Vec::as_slice)
                .unwrap_or_default(),
            delta_patches: self
                .delta_patches
                .get(&request_id)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        }
    }
}

pub(crate) async fn load_parts(
    conn: &mut PgConnection,
    ids: &[i64],
) -> Result<LoadedParts, PersistError> {
    if ids.is_empty() {
        return Ok(LoadedParts::default());
    }
    Ok(LoadedParts {
        original_messages: load_original_messages(conn, ids).await?,
        tools: load_tools(conn, ids).await?,
        sent_messages: load_sent_messages(conn, ids).await?,
        delta_patches: load_delta_patches(conn, ids).await?,
    })
}

async fn load_original_messages(
    conn: &mut PgConnection,
    ids: &[i64],
) -> Result<HashMap<i64, Vec<JsonValue>>, PersistError> {
    let rows = sqlx::query!(
        "SELECT rm.request_id, m.body, rm.cache_control
           FROM requests.messages rm
           JOIN messages.messages m ON m.id = rm.message_id
          WHERE rm.request_id = ANY($1)
          ORDER BY rm.request_id, rm.ord",
        ids,
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut out: HashMap<i64, Vec<JsonValue>> = HashMap::new();
    for row in rows {
        let mut message = row.body;
        restore_cache_control(&mut message, row.cache_control.as_ref());
        out.entry(row.request_id).or_default().push(message);
    }
    Ok(out)
}

async fn load_tools(
    conn: &mut PgConnection,
    ids: &[i64],
) -> Result<HashMap<i64, Vec<u8>>, PersistError> {
    let rows = sqlx::query!(
        "SELECT rt.request_id, d.body
           FROM requests.tools rt
           JOIN tools.definitions d ON d.id = rt.definition_id
          WHERE rt.request_id = ANY($1)
          ORDER BY rt.request_id, rt.ord",
        ids,
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut out: HashMap<i64, Vec<u8>> = HashMap::new();
    for row in rows {
        let joined = out.entry(row.request_id).or_default();
        if !joined.is_empty() {
            joined.push(b',');
        }
        joined.extend_from_slice(&row.body);
    }
    Ok(out)
}

async fn load_sent_messages(
    conn: &mut PgConnection,
    ids: &[i64],
) -> Result<HashMap<i64, Vec<(i32, JsonValue)>>, PersistError> {
    let rows = sqlx::query!(
        "SELECT sm.request_id, sm.ord, m.body, sm.cache_control
           FROM requests.sent_messages sm
           JOIN messages.messages m ON m.id = sm.message_id
          WHERE sm.request_id = ANY($1)
          ORDER BY sm.request_id, sm.ord",
        ids,
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut out: HashMap<i64, Vec<(i32, JsonValue)>> = HashMap::new();
    for row in rows {
        let mut message = row.body;
        restore_cache_control(&mut message, row.cache_control.as_ref());
        out.entry(row.request_id)
            .or_default()
            .push((row.ord, message));
    }
    Ok(out)
}

pub(super) async fn load_delta_patches(
    conn: &mut PgConnection,
    ids: &[i64],
) -> Result<HashMap<i64, Vec<SentPatchRow>>, PersistError> {
    let rows = sqlx::query!(
        r#"SELECT request_id,
                  sent_patch_start AS "sent_patch_start!",
                  sent_patch_end   AS "sent_patch_end!",
                  sent_patch_bytes AS "sent_patch_bytes!"
             FROM requests.deltas
            WHERE request_id = ANY($1)
              AND sent_patch_start IS NOT NULL"#,
        ids,
    )
    .fetch_all(&mut *conn)
    .await?;
    let mut out: HashMap<i64, Vec<SentPatchRow>> = HashMap::new();
    for row in rows {
        out.entry(row.request_id).or_default().push(SentPatchRow {
            sent_patch_start: row.sent_patch_start,
            sent_patch_end: row.sent_patch_end,
            sent_patch_bytes: row.sent_patch_bytes,
        });
    }
    Ok(out)
}

pub(crate) async fn load_stored_requests(
    conn: &mut PgConnection,
    ids: &[i64],
) -> Result<Vec<StoredRequest>, PersistError> {
    let rows = sqlx::query!(
        "SELECT id, request_envelope, sent_envelope, sent_message_count,
                sent_patch_start, sent_patch_end, sent_patch_bytes
           FROM requests.requests
          WHERE id = ANY($1)
          ORDER BY id",
        ids,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| StoredRequest {
            id: row.id,
            request_envelope: row.request_envelope,
            sent_envelope: row.sent_envelope,
            sent_message_count: row.sent_message_count,
            envelope_patch: envelope_patch(
                row.sent_patch_start,
                row.sent_patch_end,
                row.sent_patch_bytes,
            ),
        })
        .collect())
}

pub(crate) fn envelope_patch(
    start: Option<i32>,
    end: Option<i32>,
    bytes: Option<Vec<u8>>,
) -> Option<SentPatchRow> {
    match (start, end, bytes) {
        (Some(sent_patch_start), Some(sent_patch_end), Some(sent_patch_bytes)) => {
            Some(SentPatchRow {
                sent_patch_start,
                sent_patch_end,
                sent_patch_bytes,
            })
        }
        _ => None,
    }
}
