use std::collections::HashSet;

use super::PersistError;
use super::messages;
use super::session_locks;
use crate::store::models::InterceptedRequest;

async fn stamp_predecessor_and_request_number(
    tx: &mut sqlx::PgConnection,
    request_db_id: i64,
    predecessor_request_id: Option<i64>,
    session_db_id: i64,
) -> Result<(), PersistError> {
    session_locks::lock_request_numbering(&mut *tx, session_db_id).await?;
    sqlx::query!(
        "UPDATE requests.requests r
            SET predecessor_request_id = $2,
                request_number = COALESCE(r.request_number, n.next_number)
           FROM (
                SELECT COALESCE(MAX(r2.request_number), 0) + 1 AS next_number
                  FROM requests.requests r2
                 WHERE r2.session_id = $3
           ) n
          WHERE r.id = $1",
        request_db_id,
        predecessor_request_id,
        session_db_id,
    )
    .execute(&mut *tx)
    .await?;
    Ok(())
}

async fn attach_sent_patches_to_deltas(
    tx: &mut sqlx::PgConnection,
    request_db_id: i64,
    body_bytes: &[u8],
    sent_bytes: &[u8],
    canonical: &[messages::CanonicalMessage],
    delta_rows: &mut [messages::DeltaRow],
) -> Result<(), PersistError> {
    let mut patches = match messages::compute_sent_patches(body_bytes, sent_bytes, canonical) {
        Ok(patches) => patches,
        Err(e) => {
            tracing::warn!(
                request_id = request_db_id,
                error = %e,
                "insert_request_initial: sent-body patch computation skipped"
            );
            return Ok(());
        }
    };

    let added_ords: HashSet<i32> = delta_rows
        .iter()
        .filter(|r| r.change_kind == "added")
        .map(|r| r.ord)
        .collect();
    let has_homeless_diff = patches.keys().any(|ord| !added_ords.contains(ord));

    if has_homeless_diff {
        return apply_whole_body_envelope_patch(tx, request_db_id, body_bytes, sent_bytes).await;
    }

    attach_added_row_patches(delta_rows, &mut patches);
    Ok(())
}

async fn apply_whole_body_envelope_patch(
    tx: &mut sqlx::PgConnection,
    request_db_id: i64,
    body_bytes: &[u8],
    sent_bytes: &[u8],
) -> Result<(), PersistError> {
    match messages::whole_body_envelope_patch(body_bytes, sent_bytes) {
        Ok(Some(p)) => {
            sqlx::query!(
                "UPDATE requests.requests
                                SET sent_patch_start = $2,
                                    sent_patch_end   = $3,
                                    sent_patch_bytes = $4
                              WHERE id = $1",
                request_db_id,
                p.start,
                p.end,
                p.bytes,
            )
            .execute(&mut *tx)
            .await?;
        }
        Ok(None) => {}
        Err(e) => {
            tracing::warn!(
                request_id = request_db_id,
                error = %e,
                "insert_request_initial: whole-body sent-patch computation skipped"
            );
        }
    }
    Ok(())
}

fn attach_added_row_patches(
    delta_rows: &mut [messages::DeltaRow],
    patches: &mut std::collections::HashMap<i32, messages::SentPatch>,
) {
    for row in delta_rows.iter_mut() {
        if row.change_kind == "added" {
            row.sent_patch = patches.remove(&row.ord);
        }
    }
}

pub(crate) async fn write_message_dedup_bridge(
    tx: &mut sqlx::PgConnection,
    req: &InterceptedRequest,
    session_db_id: i64,
    request_db_id: i64,
) -> Result<(), PersistError> {
    let Some(body_bytes) = req.request_body.as_ref() else {
        return Ok(());
    };
    let canonical = match messages::canonicalise_body_messages(body_bytes) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                request_id = request_db_id,
                error = %e,
                "insert_request_initial: messages-bridge canonicalisation skipped"
            );
            Vec::new()
        }
    };
    if canonical.is_empty() {
        return Ok(());
    }

    let messages::PredecessorDelta {
        predecessor_request_id,
        rows: mut delta_rows,
        carry_forward,
    } = messages::resolve_predecessor_and_delta(
        &mut *tx,
        session_db_id,
        req.created_at,
        &req.turn_origin,
        &canonical,
        req.agent_instance_id,
    )
    .await?;

    messages::write_request_messages(&mut *tx, request_db_id, &canonical, &carry_forward).await?;

    stamp_predecessor_and_request_number(tx, request_db_id, predecessor_request_id, session_db_id)
        .await?;

    if let Some(sent_bytes) = req.sent_body.as_deref() {
        attach_sent_patches_to_deltas(
            tx,
            request_db_id,
            body_bytes,
            sent_bytes,
            &canonical,
            &mut delta_rows,
        )
        .await?;
    }

    messages::write_request_deltas(
        &mut *tx,
        request_db_id,
        predecessor_request_id,
        session_db_id,
        &delta_rows,
    )
    .await?;
    Ok(())
}
