pub mod span;

use super::PersistError;
use serde_json::Value as JsonValue;
use sqlx::PgPool;

use span::top_level_array_span;

const BULK_ARRAYS: [&str; 2] = ["messages", "tools"];

pub fn strip_bulk_arrays(body: &[u8]) -> Option<Vec<u8>> {
    if body.first() != Some(&b'{') {
        return None;
    }
    let mut spans: Vec<(usize, usize)> = BULK_ARRAYS
        .iter()
        .filter_map(|key| top_level_array_span(body, key))
        .filter(|(start, end)| end > start)
        .collect();
    if spans.is_empty() {
        return Some(body.to_vec());
    }
    spans.sort_by_key(|(start, _)| std::cmp::Reverse(*start));
    let mut out = body.to_vec();
    for (start, end) in spans {
        out.drain(start..end);
    }
    Some(out)
}

pub async fn rehydrate_body(
    pool: &PgPool,
    request_id: i64,
    body: Option<bytes::Bytes>,
    envelope: Option<bytes::Bytes>,
) -> Result<Option<ServedBody>, PersistError> {
    if let Some(body) = body {
        return Ok(Some(ServedBody {
            bytes: body,
            reconstructed: false,
        }));
    }
    let Some(envelope) = envelope else {
        return Ok(None);
    };

    let mut out = envelope.to_vec();
    let mut splices: Vec<(usize, Vec<u8>)> = Vec::new();
    for key in BULK_ARRAYS {
        let Some((at, end)) = top_level_array_span(&out, key) else {
            continue;
        };
        debug_assert_eq!(at, end, "the envelope's bulk arrays are stored empty");
        let contents = match key {
            "messages" => rebuild_messages(pool, request_id).await?,
            "tools" => rebuild_tools(pool, request_id).await?,
            _ => unreachable!("BULK_ARRAYS is exhaustive"),
        };
        if !contents.is_empty() {
            splices.push((at, contents));
        }
    }
    splices.sort_by_key(|(at, _)| std::cmp::Reverse(*at));
    for (at, contents) in splices {
        out.splice(at..at, contents);
    }
    Ok(Some(ServedBody {
        bytes: bytes::Bytes::from(out),
        reconstructed: true,
    }))
}

pub struct ServedBody {
    pub bytes: bytes::Bytes,
    pub reconstructed: bool,
}

async fn rebuild_messages(pool: &PgPool, request_id: i64) -> Result<Vec<u8>, PersistError> {
    let rows = sqlx::query!(
        "SELECT m.body, rm.cache_control
           FROM requests.messages rm
           JOIN messages.messages m ON m.id = rm.message_id
          WHERE rm.request_id = $1
          ORDER BY rm.ord ASC",
        request_id,
    )
    .fetch_all(pool)
    .await?;

    let mut out = Vec::new();
    for (idx, row) in rows.into_iter().enumerate() {
        let mut message = row.body;
        restore_cache_control(&mut message, row.cache_control.as_ref());
        if idx > 0 {
            out.push(b',');
        }
        serde_json::to_writer(&mut out, &message)?;
    }
    Ok(out)
}

async fn rebuild_tools(pool: &PgPool, request_id: i64) -> Result<Vec<u8>, PersistError> {
    let rows = sqlx::query_scalar!(
        "SELECT d.body
           FROM requests.tools rt
           JOIN tools.definitions d ON d.id = rt.definition_id
          WHERE rt.request_id = $1
          ORDER BY rt.ord ASC",
        request_id,
    )
    .fetch_all(pool)
    .await?;

    let mut out = Vec::new();
    for (idx, definition) in rows.into_iter().enumerate() {
        if idx > 0 {
            out.push(b',');
        }
        out.extend_from_slice(&definition);
    }
    Ok(out)
}

fn restore_cache_control(message: &mut JsonValue, recorded: Option<&JsonValue>) {
    let Some(JsonValue::Array(entries)) = recorded else {
        return;
    };
    for entry in entries {
        let (Some(pointer), Some(value)) = (
            entry.get("pointer").and_then(JsonValue::as_str),
            entry.get("value"),
        ) else {
            continue;
        };
        let Some((parent_pointer, last)) = pointer.rsplit_once('/') else {
            continue;
        };
        if last != "cache_control" {
            continue;
        }
        let parent = if parent_pointer.is_empty() {
            Some(&mut *message)
        } else {
            message.pointer_mut(parent_pointer)
        };
        if let Some(JsonValue::Object(map)) = parent {
            map.insert("cache_control".to_string(), value.clone());
        }
    }
}

async fn fetch_envelope_candidates(
    pool: &PgPool,
    cursor: i64,
    to: Option<i64>,
    limit: i64,
) -> Result<Vec<EnvelopeCandidate>, PersistError> {
    let rows = match to {
        Some(to) => {
            sqlx::query_as!(
                EnvelopeCandidate,
                r#"SELECT id, request_body AS "request_body!"
                     FROM requests.requests
                    WHERE id >= $1
                      AND id <= $2
                      AND request_body IS NOT NULL
                      AND request_envelope IS NULL
                    ORDER BY id ASC
                    LIMIT $3"#,
                cursor,
                to,
                limit,
            )
            .fetch_all(pool)
            .await?
        }
        None => {
            sqlx::query_as!(
                EnvelopeCandidate,
                r#"SELECT id, request_body AS "request_body!"
                     FROM requests.requests
                    WHERE id >= $1
                      AND request_body IS NOT NULL
                      AND request_envelope IS NULL
                    ORDER BY id ASC
                    LIMIT $2"#,
                cursor,
                limit,
            )
            .fetch_all(pool)
            .await?
        }
    };
    Ok(rows)
}

pub async fn backfill_envelope_batch(
    pool: &PgPool,
    cursor: i64,
    to: Option<i64>,
    limit: i64,
) -> Result<Option<EnvelopeBatchOutcome>, PersistError> {
    let rows = fetch_envelope_candidates(pool, cursor, to, limit).await?;

    let Some(watermark) = rows.last().map(|row| row.id) else {
        return Ok(None);
    };

    let scanned = rows.len();
    let batch = EnvelopeBackfillInputs::from_candidates(rows);
    let unparseable = scanned - batch.ids.len();
    let updated = write_envelopes(pool, &batch).await?;
    let cache_control_recovered = recover_cache_control(pool, &batch).await?;

    Ok(Some(EnvelopeBatchOutcome {
        watermark,
        scanned,
        updated,
        unparseable,
        cache_control_recovered,
    }))
}

#[derive(Default)]
struct EnvelopeBackfillInputs {
    ids: Vec<i64>,
    envelopes: Vec<Vec<u8>>,
    cc_request_ids: Vec<i64>,
    cc_ords: Vec<i32>,
    cc_values: Vec<JsonValue>,
}

impl EnvelopeBackfillInputs {
    fn from_candidates(rows: Vec<EnvelopeCandidate>) -> Self {
        let mut batch = Self::default();
        for EnvelopeCandidate {
            id,
            request_body: body,
        } in rows
        {
            if let Some(envelope) = strip_bulk_arrays(&body) {
                batch.ids.push(id);
                batch.envelopes.push(envelope);
            }
            batch.collect_cache_controls(id, &body);
        }
        batch
    }

    fn collect_cache_controls(&mut self, id: i64, body: &[u8]) {
        let Ok(canonical) = super::messages::canonicalise_body_messages(body) else {
            return;
        };
        for (ord, message) in canonical.iter().enumerate() {
            if let Some(cache_control) = message.cache_control.clone() {
                self.cc_request_ids.push(id);
                self.cc_ords.push(ord as i32);
                self.cc_values.push(cache_control);
            }
        }
    }
}

async fn write_envelopes(
    pool: &PgPool,
    batch: &EnvelopeBackfillInputs,
) -> Result<usize, PersistError> {
    if batch.ids.is_empty() {
        return Ok(0);
    }
    let affected = sqlx::query!(
        "UPDATE requests.requests r
                SET request_envelope = v.envelope
               FROM (SELECT UNNEST($1::bigint[]) AS id, UNNEST($2::bytea[]) AS envelope) v
              WHERE r.id = v.id",
        &batch.ids,
        &batch.envelopes,
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok(affected as usize)
}

async fn recover_cache_control(
    pool: &PgPool,
    batch: &EnvelopeBackfillInputs,
) -> Result<usize, PersistError> {
    if batch.cc_request_ids.is_empty() {
        return Ok(0);
    }
    let affected = sqlx::query!(
        "UPDATE requests.messages rm
                SET cache_control = v.cache_control
               FROM (SELECT UNNEST($1::bigint[]) AS request_id,
                            UNNEST($2::int[])    AS ord,
                            UNNEST($3::jsonb[])  AS cache_control) v
              WHERE rm.request_id = v.request_id
                AND rm.ord = v.ord
                AND rm.cache_control IS NULL",
        &batch.cc_request_ids,
        &batch.cc_ords,
        &batch.cc_values,
    )
    .execute(pool)
    .await?
    .rows_affected();
    Ok(affected as usize)
}

struct EnvelopeCandidate {
    id: i64,
    request_body: Vec<u8>,
}

pub struct EnvelopeBatchOutcome {
    pub watermark: i64,
    pub scanned: usize,
    pub updated: usize,
    pub unparseable: usize,
    pub cache_control_recovered: usize,
}

#[cfg(test)]
mod tests;
