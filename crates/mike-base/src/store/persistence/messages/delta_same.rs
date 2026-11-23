use sqlx::PgConnection;

use super::DeltaRow;
use crate::store::persistence::PersistError;

type Key = (String, i32, Option<String>);

fn keys(rows: &[DeltaRow]) -> Vec<Key> {
    let mut keys: Vec<Key> = rows
        .iter()
        .map(|r| {
            (
                r.change_kind.to_string(),
                r.ord,
                r.removed_reason.map(str::to_owned),
            )
        })
        .collect();
    keys.sort();
    keys
}

/// What a request already has stored, against the delta rows about to be written.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StoredDeltas {
    /// No rows yet: write without clearing.
    None,
    /// The same rows: nothing to write.
    Same,
    /// Other rows: clear them, then write.
    Different,
}

pub async fn stored_deltas(
    conn: &mut PgConnection,
    request_id: i64,
    predecessor_request_id: Option<i64>,
    session_db_id: i64,
    rows: &[DeltaRow],
) -> Result<StoredDeltas, PersistError> {
    let stored = sqlx::query!(
        r#"SELECT dck.code AS "kind!", d.ord AS "ord!", drr.code AS "reason?",
                  d.predecessor_request_id, d.session_id
             FROM requests.deltas d
             JOIN requests.delta_change_kinds dck ON dck.id = d.change_kind_id
             LEFT JOIN requests.delta_removed_reasons drr ON drr.id = d.removed_reason_id
            WHERE d.request_id = $1"#,
        request_id
    )
    .fetch_all(&mut *conn)
    .await?;
    if stored.is_empty() {
        return Ok(if rows.is_empty() {
            StoredDeltas::Same
        } else {
            StoredDeltas::None
        });
    }
    if stored.len() != rows.len() {
        return Ok(StoredDeltas::Different);
    }
    let same_links = stored.iter().all(|s| {
        s.predecessor_request_id == predecessor_request_id && s.session_id == session_db_id
    });
    if !same_links {
        return Ok(StoredDeltas::Different);
    }
    let mut existing: Vec<Key> = stored
        .into_iter()
        .map(|s| (s.kind, s.ord, s.reason))
        .collect();
    existing.sort();
    Ok(if existing == keys(rows) {
        StoredDeltas::Same
    } else {
        StoredDeltas::Different
    })
}
