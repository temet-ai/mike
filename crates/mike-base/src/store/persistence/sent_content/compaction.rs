use std::collections::HashMap;

use sqlx::PgConnection;

use super::assemble::{RequestParts, StoredRequest, assemble_original, assemble_sent, same_json};
use super::parts::{load_parts, load_stored_requests};
use crate::store::persistence::PersistError;
use crate::store::persistence::messages::apply_sent_patches;

pub(crate) struct ClearCandidate {
    pub id: i64,
    pub request_body: Vec<u8>,
    pub has_filtered_body: bool,
}

pub(crate) async fn clearable_ids(
    conn: &mut PgConnection,
    candidates: &[ClearCandidate],
) -> Result<Vec<i64>, PersistError> {
    let ids: Vec<i64> = candidates.iter().map(|c| c.id).collect();
    let stored: HashMap<i64, StoredRequest> = load_stored_requests(conn, &ids)
        .await?
        .into_iter()
        .map(|request| (request.id, request))
        .collect();
    let loaded = load_parts(conn, &ids).await?;
    Ok(candidates
        .iter()
        .filter(|candidate| {
            stored.get(&candidate.id).is_some_and(|request| {
                rebuilds_without_body(candidate, request, loaded.for_request(candidate.id))
            })
        })
        .map(|candidate| candidate.id)
        .collect())
}

fn rebuilds_without_body(
    candidate: &ClearCandidate,
    request: &StoredRequest,
    parts: RequestParts<'_>,
) -> bool {
    let Some(envelope) = request.request_envelope.as_deref() else {
        return false;
    };
    let original_matches = assemble_original(envelope, parts)
        .is_ok_and(|original| same_json(&original, &candidate.request_body));
    original_matches && (candidate.has_filtered_body || sent_matches(candidate, request, parts))
}

fn sent_matches(
    candidate: &ClearCandidate,
    request: &StoredRequest,
    parts: RequestParts<'_>,
) -> bool {
    let Ok(Some(rebuilt)) = assemble_sent(request, parts, None) else {
        return false;
    };
    if request.has_legacy_patches(parts) {
        return apply_sent_patches(&candidate.request_body, request.legacy_patches(parts))
            .is_ok_and(|expected| same_json(&rebuilt, &expected));
    }
    if request.has_sent_content(parts) {
        return serde_json::from_slice::<serde_json::Value>(&rebuilt).is_ok();
    }
    same_json(&rebuilt, &candidate.request_body)
}
