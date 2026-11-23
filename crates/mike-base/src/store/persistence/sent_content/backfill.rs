use std::collections::HashMap;

use sqlx::PgConnection;

use super::assemble::{assemble_sent, same_json};
use super::backfill_derive::{DerivedSent, LegacyRow, derive_from_legacy};
use super::parts::{envelope_patch, load_delta_patches, load_parts, load_stored_requests};
use super::store::{
    BridgedMessage, SentRow, bridged_messages, clear_sent_content, insert_sent_messages,
    sent_message_ids, set_sent_envelopes, set_sent_message_counts,
};
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentBackfillFailure {
    pub request_id: i64,
    pub reason: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SentBackfillBatch {
    pub watermark: i64,
    pub scanned: usize,
    pub verified: usize,
    pub failures: Vec<SentBackfillFailure>,
}

struct Candidate {
    id: i64,
    request_body: Vec<u8>,
    request_envelope: Option<Vec<u8>>,
    predecessor_request_id: Option<i64>,
    sent_patch_start: Option<i32>,
    sent_patch_end: Option<i32>,
    sent_patch_bytes: Option<Vec<u8>>,
}

impl PersistHandle {
    pub async fn backfill_sent_content_batch(
        &self,
        cursor: i64,
        to: Option<i64>,
        limit: i64,
        execute: bool,
    ) -> Result<Option<SentBackfillBatch>, PersistError> {
        crate::metrics::time_db("backfill_sent_content_batch", async move {
            let mut tx = self.background_pool().begin().await?;
            sqlx::query!("SET LOCAL statement_timeout = '120s'")
                .execute(&mut *tx)
                .await?;
            let candidates =
                fetch_candidates(&mut tx, cursor, to.unwrap_or(i64::MAX), limit).await?;
            let Some(watermark) = candidates.last().map(|c| c.id) else {
                tx.rollback().await?;
                return Ok(None);
            };
            let mut batch = SentBackfillBatch {
                watermark,
                scanned: candidates.len(),
                ..SentBackfillBatch::default()
            };
            let ids: Vec<i64> = candidates.iter().map(|c| c.id).collect();
            let bridged = bridged_messages(&mut tx, &ids).await?;
            let derived = derive_batch(&mut tx, &candidates, &bridged, &mut batch.failures).await?;
            write_derived(&mut tx, &candidates, &derived, &bridged).await?;
            let failed = verify_derived(&mut tx, &derived, &mut batch.failures).await?;
            clear_sent_content(&mut tx, &failed).await?;
            batch.verified = derived.len() - failed.len();
            if execute {
                tx.commit().await?;
            } else {
                tx.rollback().await?;
            }
            Ok(Some(batch))
        })
        .await
    }
}

async fn fetch_candidates(
    conn: &mut PgConnection,
    cursor: i64,
    to: i64,
    limit: i64,
) -> Result<Vec<Candidate>, PersistError> {
    let rows = sqlx::query_as!(
        Candidate,
        r#"SELECT r.id, r.request_body AS "request_body!", r.request_envelope,
                  r.predecessor_request_id,
                  r.sent_patch_start, r.sent_patch_end, r.sent_patch_bytes
             FROM requests.requests r
            WHERE r.id >= $1
              AND r.id <= $2
              AND r.request_body IS NOT NULL
              AND r.filtered_request_body IS NULL
              AND r.sent_envelope IS NULL
              AND r.sent_message_count IS NULL
              AND (r.sent_patch_start IS NOT NULL
                   OR EXISTS (SELECT 1 FROM requests.deltas d
                               WHERE d.request_id = r.id
                                 AND d.sent_patch_start IS NOT NULL))
              AND NOT EXISTS (SELECT 1 FROM requests.sent_messages sm
                               WHERE sm.request_id = r.id)
            ORDER BY r.id
            LIMIT $3"#,
        cursor,
        to,
        limit,
    )
    .fetch_all(&mut *conn)
    .await?;
    Ok(rows)
}

async fn derive_batch(
    conn: &mut PgConnection,
    candidates: &[Candidate],
    bridged: &HashMap<i64, Vec<BridgedMessage>>,
    failures: &mut Vec<SentBackfillFailure>,
) -> Result<HashMap<i64, DerivedSent>, PersistError> {
    let ids: Vec<i64> = candidates.iter().map(|c| c.id).collect();
    let mut delta_patches = load_delta_patches(conn, &ids).await?;
    let mut derived = HashMap::with_capacity(candidates.len());
    for candidate in candidates {
        let mut patches = delta_patches.remove(&candidate.id).unwrap_or_default();
        patches.extend(envelope_patch(
            candidate.sent_patch_start,
            candidate.sent_patch_end,
            candidate.sent_patch_bytes.clone(),
        ));
        let row = LegacyRow {
            raw_body: &candidate.request_body,
            request_envelope: candidate.request_envelope.as_deref(),
            patches,
            bridged: bridged
                .get(&candidate.id)
                .map(Vec::as_slice)
                .unwrap_or_default(),
        };
        match derive_from_legacy(row) {
            Ok(sent) => {
                derived.insert(candidate.id, sent);
            }
            Err(reason) => failures.push(SentBackfillFailure {
                request_id: candidate.id,
                reason,
            }),
        }
    }
    Ok(derived)
}

async fn write_derived(
    conn: &mut PgConnection,
    candidates: &[Candidate],
    derived: &HashMap<i64, DerivedSent>,
    bridged: &HashMap<i64, Vec<BridgedMessage>>,
) -> Result<(), PersistError> {
    let mut envelope_ids = Vec::new();
    let mut envelopes = Vec::new();
    let mut count_ids = Vec::new();
    let mut counts = Vec::new();
    let mut rows = Vec::new();
    for (id, sent) in derived {
        if let Some(envelope) = &sent.envelope {
            envelope_ids.push(*id);
            envelopes.push(envelope.clone());
        }
        if let Some(count) = sent.count {
            count_ids.push(*id);
            counts.push(count);
        }
        rows.extend(sent.rows.iter().map(|message| SentRow {
            request_id: *id,
            message,
        }));
    }
    set_sent_envelopes(conn, &envelope_ids, &envelopes).await?;
    set_sent_message_counts(conn, &count_ids, &counts).await?;
    if rows.is_empty() {
        return Ok(());
    }
    let known = known_message_ids(conn, candidates, derived, bridged).await?;
    insert_sent_messages(conn, &rows, &known).await
}

async fn known_message_ids(
    conn: &mut PgConnection,
    candidates: &[Candidate],
    derived: &HashMap<i64, DerivedSent>,
    bridged: &HashMap<i64, Vec<BridgedMessage>>,
) -> Result<HashMap<String, i64>, PersistError> {
    let with_rows: Vec<&Candidate> = candidates
        .iter()
        .filter(|c| derived.get(&c.id).is_some_and(|d| !d.rows.is_empty()))
        .collect();
    let predecessors: Vec<i64> = with_rows
        .iter()
        .filter_map(|c| c.predecessor_request_id)
        .collect();
    let mut known = sent_message_ids(conn, &predecessors).await?;
    for candidate in with_rows {
        let messages = bridged
            .get(&candidate.id)
            .map(Vec::as_slice)
            .unwrap_or_default();
        known.extend(messages.iter().map(|m| (m.sha.clone(), m.message_id)));
    }
    Ok(known)
}

async fn verify_derived(
    conn: &mut PgConnection,
    derived: &HashMap<i64, DerivedSent>,
    failures: &mut Vec<SentBackfillFailure>,
) -> Result<Vec<i64>, PersistError> {
    let mut ids: Vec<i64> = derived.keys().copied().collect();
    ids.sort_unstable();
    let stored = load_stored_requests(conn, &ids).await?;
    let loaded = load_parts(conn, &ids).await?;
    let mut failed = Vec::new();
    for request in &stored {
        let Some(expected) = derived.get(&request.id) else {
            continue;
        };
        let rebuilt = assemble_sent(request, loaded.for_request(request.id), None);
        let reason = match rebuilt {
            Ok(Some(body)) if same_json(&body, &expected.true_sent) => continue,
            Ok(Some(_)) => {
                "the rebuilt sent body differs from request_body plus its patches".to_string()
            }
            Ok(None) => "the rebuilt sent body is missing".to_string(),
            Err(e) => format!("the sent body does not rebuild: {e}"),
        };
        failed.push(request.id);
        failures.push(SentBackfillFailure {
            request_id: request.id,
            reason,
        });
    }
    Ok(failed)
}
