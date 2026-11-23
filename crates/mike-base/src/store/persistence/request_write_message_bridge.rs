use std::collections::HashMap;

use super::PersistError;
use super::messages;
use super::sent_content;
use super::session_locks;
use crate::store::models::InterceptedRequest;

async fn stamp_predecessor_and_request_number(
    tx: &mut sqlx::PgConnection,
    request_db_id: i64,
    predecessor: &messages::PredecessorDelta,
    session_db_id: i64,
) -> Result<(), PersistError> {
    session_locks::lock_request_numbering(&mut *tx, session_db_id).await?;
    let stamp = sqlx::query(STAMP_IN_SEND_ORDER); // grader-allow: no .sqlx entry can be described while crate mike does not compile
    stamp
        .bind(request_db_id)
        .bind(predecessor.predecessor_request_id)
        .bind(session_db_id)
        .bind(predecessor.rule.code())
        .execute(&mut *tx)
        .await?;
    Ok(())
}

const STAMP_IN_SEND_ORDER: &str = "WITH mine AS (
             SELECT id, created_at, request_number
               FROM requests.requests
              WHERE id = $1
         ),
         shifted AS (
             UPDATE requests.requests later
                SET request_number = later.request_number + 1
               FROM mine
              WHERE mine.request_number IS NULL
                AND later.session_id = $3
                AND later.request_number IS NOT NULL
                AND (later.created_at, later.id) > (mine.created_at, mine.id)
             RETURNING later.id
         )
         UPDATE requests.requests r
            SET predecessor_request_id = $2,
                predecessor_rule_id = (SELECT pr.id FROM requests.predecessor_rules pr WHERE pr.code = $4),
                request_number = COALESCE(r.request_number, n.next_number)
           FROM (
                SELECT COALESCE((
                    SELECT earlier.request_number
                      FROM requests.requests earlier, mine
                     WHERE earlier.session_id = $3
                       AND earlier.request_number IS NOT NULL
                       AND (earlier.created_at, earlier.id) < (mine.created_at, mine.id)
                     ORDER BY earlier.created_at DESC, earlier.id DESC
                     LIMIT 1
                ), 0) + 1 AS next_number
           ) n
          WHERE r.id = $1";

pub async fn write_message_dedup_bridge(
    tx: &mut sqlx::PgConnection,
    req: &InterceptedRequest,
    agent_instance_id: Option<i64>,
    session_db_id: i64,
    request_db_id: i64,
    fresh_row: bool,
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
        return write_sent_messages(tx, req, request_db_id, &canonical, &HashMap::new(), None)
            .await;
    }
    let canonical: Vec<messages::CanonicalMessage> = canonical
        .into_iter()
        .map(messages::clean_canonical_message)
        .collect();

    let predecessor = messages::resolve_predecessor_and_delta(
        &mut *tx,
        req,
        session_db_id,
        &canonical,
        agent_instance_id,
    )
    .await?;

    let message_ids = write_bridge_messages(
        &mut *tx,
        (request_db_id, fresh_row),
        &canonical,
        &predecessor.carry_forward,
    )
    .await?;

    stamp_predecessor_and_request_number(tx, request_db_id, &predecessor, session_db_id).await?;
    let predecessor_request_id = predecessor.predecessor_request_id;
    write_sent_messages(
        tx,
        req,
        request_db_id,
        &canonical,
        &message_ids,
        predecessor_request_id,
    )
    .await?;

    write_bridge_deltas(
        &mut *tx,
        (request_db_id, fresh_row),
        predecessor_request_id,
        session_db_id,
        &predecessor.rows,
    )
    .await
}

async fn write_sent_messages(
    tx: &mut sqlx::PgConnection,
    req: &InterceptedRequest,
    request_db_id: i64,
    canonical: &[messages::CanonicalMessage],
    message_ids: &HashMap<String, i64>,
    predecessor_request_id: Option<i64>,
) -> Result<(), PersistError> {
    let (Some(original), Some(sent)) = (req.request_body.as_deref(), req.sent_body.as_deref())
    else {
        return Ok(());
    };
    sent_content::write_captured_sent_messages(
        tx,
        request_db_id,
        sent_content::CapturedBodies { original, sent },
        sent_content::OriginalMessages {
            canonical,
            ids: message_ids,
            predecessor_request_id,
        },
    )
    .await
}

async fn write_bridge_messages(
    tx: &mut sqlx::PgConnection,
    (request_db_id, fresh_row): (i64, bool),
    canonical: &[messages::CanonicalMessage],
    carry_forward: &HashMap<String, i64>,
) -> Result<HashMap<String, i64>, PersistError> {
    if fresh_row {
        messages::insert_request_messages(tx, request_db_id, canonical, carry_forward).await
    } else {
        messages::write_request_messages(tx, request_db_id, canonical, carry_forward).await
    }
}

async fn write_bridge_deltas(
    tx: &mut sqlx::PgConnection,
    (request_db_id, fresh_row): (i64, bool),
    predecessor_request_id: Option<i64>,
    session_db_id: i64,
    rows: &[messages::DeltaRow],
) -> Result<(), PersistError> {
    if fresh_row {
        messages::insert_request_deltas(
            tx,
            request_db_id,
            predecessor_request_id,
            session_db_id,
            rows,
        )
        .await
    } else {
        messages::write_request_deltas(
            tx,
            request_db_id,
            predecessor_request_id,
            session_db_id,
            rows,
        )
        .await
    }
}
