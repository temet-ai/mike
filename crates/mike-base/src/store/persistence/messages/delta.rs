use sqlx::PgConnection;

use crate::store::persistence::PersistError;

use super::canonicalize::CanonicalMessage;

pub struct DeltaRow {
    pub change_kind: &'static str,
    pub ord: i32,
    pub removed_reason: Option<&'static str>,
    pub sent_patch: Option<SentPatch>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentPatch {
    pub start: i32,
    pub end: i32,
    pub bytes: Vec<u8>,
}

pub const COMPACTION_CONTINUATION_MARKER: &str =
    "This session is being continued from a previous conversation";

pub const MARKER_LEADING_WINDOW_BYTES: usize = 200;

pub fn is_reliable_sidecar_origin(turn_origin: &str) -> bool {
    super::RELIABLE_SIDECAR_ORIGINS.contains(&turn_origin)
}

fn current_is_summary_shaped(current: &[CanonicalMessage]) -> bool {
    current
        .first()
        .map(|m| {
            let window_end = m
                .body_canonical_bytes
                .len()
                .min(MARKER_LEADING_WINDOW_BYTES);
            let leading = &m.body_canonical_bytes[..window_end];
            memchr_contains(leading, COMPACTION_CONTINUATION_MARKER.as_bytes())
        })
        .unwrap_or(false)
}

fn memchr_contains(haystack: &[u8], needle: &[u8]) -> bool {
    if needle.is_empty() || needle.len() > haystack.len() {
        return needle.is_empty();
    }
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

pub fn compute_delta(
    current: &[CanonicalMessage],
    predecessor: Option<&[(i32, String)]>,
) -> Vec<DeltaRow> {
    use std::collections::HashSet;

    let pred: &[(i32, String)] = predecessor.unwrap_or(&[]);
    let pred_shas: HashSet<&str> = pred.iter().map(|(_, s)| s.as_str()).collect();
    let current_shas: HashSet<&str> = current.iter().map(|m| m.sha.as_str()).collect();

    let mut rows: Vec<DeltaRow> = Vec::with_capacity(current.len());

    for m in current {
        if !pred_shas.contains(m.sha.as_str()) {
            rows.push(DeltaRow {
                change_kind: "added",
                ord: m.ord,
                removed_reason: None,
                sent_patch: None,
            });
        }
    }

    let removed_positions: Vec<(i32, &str)> = pred
        .iter()
        .filter(|(_, sha)| !current_shas.contains(sha.as_str()))
        .map(|(ord, sha)| (*ord, sha.as_str()))
        .collect();

    if !removed_positions.is_empty() {
        let reason = classify_removed(pred, &removed_positions, current);
        for (ord, _sha) in &removed_positions {
            rows.push(DeltaRow {
                change_kind: "removed",
                ord: *ord,
                removed_reason: Some(reason),
                sent_patch: None,
            });
        }
    }

    rows
}

pub fn classify_removed(
    predecessor: &[(i32, String)],
    removed: &[(i32, &str)],
    current: &[CanonicalMessage],
) -> &'static str {
    if removed.is_empty() {
        return "unknown";
    }

    let whole_bridge_wipe = removed.len() == predecessor.len() && !predecessor.is_empty();
    if whole_bridge_wipe {
        let pred_shas: std::collections::HashSet<&str> =
            predecessor.iter().map(|(_, s)| s.as_str()).collect();
        let any_sha_overlap = current.iter().any(|m| pred_shas.contains(m.sha.as_str()));
        if !any_sha_overlap && !current_is_summary_shaped(current) {
            return "unknown";
        }
    }

    let mut removed_ords: Vec<i32> = removed.iter().map(|(o, _)| *o).collect();
    removed_ords.sort_unstable();

    let min_removed = removed_ords[0];
    let max_removed = removed_ords[removed_ords.len() - 1];
    let len = removed_ords.len() as i32;
    let is_contiguous = (max_removed - min_removed + 1) == len;

    if !is_contiguous {
        return "unknown";
    }

    let pred_max_ord = predecessor.iter().map(|(o, _)| *o).max().unwrap_or(0);

    let is_prefix = min_removed == 0;
    let current_first_is_new = current
        .first()
        .map(|m| !predecessor.iter().any(|(_, sha)| sha == &m.sha))
        .unwrap_or(false);
    if is_prefix && current_first_is_new && current_is_summary_shaped(current) {
        return "compaction";
    }

    if max_removed == pred_max_ord {
        return "tail_rebuild";
    }

    "unknown"
}

pub async fn write_request_deltas(
    conn: &mut PgConnection,
    request_id: i64,
    predecessor_request_id: Option<i64>,
    session_db_id: i64,
    rows: &[DeltaRow],
) -> Result<(), PersistError> {
    sqlx::query!(
        "DELETE FROM requests.deltas WHERE request_id = $1",
        request_id
    )
    .execute(&mut *conn)
    .await?;

    if rows.is_empty() {
        return Ok(());
    }

    let kinds: Vec<String> = rows.iter().map(|r| r.change_kind.to_string()).collect();
    let ords: Vec<i32> = rows.iter().map(|r| r.ord).collect();
    let reasons: Vec<Option<String>> = rows
        .iter()
        .map(|r| r.removed_reason.map(str::to_owned))
        .collect();
    let patch_starts: Vec<Option<i32>> = rows
        .iter()
        .map(|r| r.sent_patch.as_ref().map(|p| p.start))
        .collect();
    let patch_ends: Vec<Option<i32>> = rows
        .iter()
        .map(|r| r.sent_patch.as_ref().map(|p| p.end))
        .collect();
    let patch_bytes: Vec<Option<Vec<u8>>> = rows
        .iter()
        .map(|r| r.sent_patch.as_ref().map(|p| p.bytes.clone()))
        .collect();

    sqlx::query!(
        "INSERT INTO requests.deltas (
            request_id, predecessor_request_id,
            session_id,
            change_kind_id, ord,
            message_id,
            removed_reason_id,
            sent_patch_start, sent_patch_end, sent_patch_bytes
         )
         SELECT $1, $2,
                $6,
                dck.id,
                d.ord,
                m.id,
                drr.id,
                d.patch_start, d.patch_end, d.patch_bytes
           FROM UNNEST($3::text[], $4::int[], $5::text[], $7::int[], $8::int[], $9::bytea[])
                AS d(kind, ord, reason, patch_start, patch_end, patch_bytes)
           JOIN requests.delta_change_kinds         dck ON dck.code = d.kind
           LEFT JOIN requests.delta_removed_reasons drr ON drr.code = d.reason
           -- Resolve message_id from the bridge: added rows use the current
           -- request ($1); removed rows use the predecessor ($2).
           JOIN requests.messages rm
                  ON rm.request_id = CASE WHEN d.kind = 'added' THEN $1::bigint ELSE $2::bigint END
                 AND rm.ord        = d.ord
           -- m.id is rm.message_id's own row; the JOIN pins message_id to
           -- a live messages.messages row (role/byte_size/body no longer
           -- read here — the fetch side sources them from this JOIN).
           JOIN messages.messages m ON m.id = rm.message_id",
        request_id,
        predecessor_request_id,
        &kinds,
        &ords,
        &reasons as &[Option<String>],
        session_db_id,
        &patch_starts as &[Option<i32>],
        &patch_ends as &[Option<i32>],
        &patch_bytes as &[Option<Vec<u8>>],
    )
    .execute(&mut *conn)
    .await?;

    Ok(())
}
