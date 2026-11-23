use std::collections::HashMap;

use sqlx::PgConnection;

use super::derive::{MessageKey, plan_sent_messages, sent_messages_split, whole_sent_envelope};
use super::store::{
    SentRow, insert_sent_messages, sent_message_ids, set_sent_envelopes, set_sent_message_counts,
};
use crate::store::persistence::PersistError;
use crate::store::persistence::messages::{CanonicalMessage, canonical_clean_messages};

pub struct CapturedBodies<'a> {
    pub original: &'a [u8],
    pub sent: &'a [u8],
}

pub struct OriginalMessages<'a> {
    pub canonical: &'a [CanonicalMessage],
    pub ids: &'a HashMap<String, i64>,
    pub predecessor_request_id: Option<i64>,
}

pub async fn write_captured_sent_messages(
    conn: &mut PgConnection,
    request_id: i64,
    bodies: CapturedBodies<'_>,
    original: OriginalMessages<'_>,
) -> Result<(), PersistError> {
    if bodies.original == bodies.sent || !sent_messages_split(bodies.sent) {
        return Ok(());
    }
    let sent = match canonical_clean_messages(bodies.sent) {
        Ok(sent) => sent,
        Err(error) => {
            tracing::warn!(
                request_id,
                error = %error,
                "sent messages could not be canonicalised; storing the whole sent body as its envelope"
            );
            let envelope = whole_sent_envelope(bodies.original, bodies.sent);
            return set_sent_envelopes(conn, &[request_id], &[envelope]).await;
        }
    };
    let keys: Vec<MessageKey<'_>> = original.canonical.iter().map(MessageKey::from).collect();
    let plan = plan_sent_messages(&keys, sent);
    if let Some(count) = plan.count {
        set_sent_message_counts(conn, &[request_id], &[count]).await?;
    }
    if plan.rows.is_empty() {
        return Ok(());
    }
    let mut known = original.ids.clone();
    let unresolved = plan.rows.iter().any(|m| !known.contains_key(&m.sha));
    if unresolved && let Some(predecessor) = original.predecessor_request_id {
        known.extend(sent_message_ids(conn, &[predecessor]).await?);
    }
    let rows: Vec<SentRow<'_>> = plan
        .rows
        .iter()
        .map(|message| SentRow {
            request_id,
            message,
        })
        .collect();
    insert_sent_messages(conn, &rows, &known).await
}
