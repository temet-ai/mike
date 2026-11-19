use sqlx::PgConnection;

use super::DeltaRow;
use crate::store::persistence::PersistError;

type Key = (String, i32, Option<String>, Option<i32>, Option<i32>, Option<Vec<u8>>);

fn keys(rows: &[DeltaRow]) -> Vec<Key> {
    let mut keys: Vec<Key> = rows
        .iter()
        .map(|r| {
            (
                r.change_kind.to_string(),
                r.ord,
                r.removed_reason.map(str::to_owned),
                r.sent_patch.as_ref().map(|p| p.start),
                r.sent_patch.as_ref().map(|p| p.end),
                r.sent_patch.as_ref().map(|p| p.bytes.clone()),
            )
        })
        .collect();
    keys.sort();
    keys
}

pub async fn deltas_unchanged(
    conn: &mut PgConnection,
    request_id: i64,
    predecessor_request_id: Option<i64>,
    session_db_id: i64,
    rows: &[DeltaRow],
) -> Result<bool, PersistError> {
    let stored = sqlx::query!(
        r#"SELECT dck.code AS "kind!", d.ord AS "ord!", drr.code AS "reason?",
                  d.sent_patch_start, d.sent_patch_end, d.sent_patch_bytes,
                  d.predecessor_request_id, d.session_id
             FROM requests.deltas d
             JOIN requests.delta_change_kinds dck ON dck.id = d.change_kind_id
             LEFT JOIN requests.delta_removed_reasons drr ON drr.id = d.removed_reason_id
            WHERE d.request_id = $1"#,
        request_id
    )
    .fetch_all(&mut *conn)
    .await?;
    if stored.len() != rows.len() {
        return Ok(false);
    }
    let same_links = stored
        .iter()
        .all(|s| s.predecessor_request_id == predecessor_request_id && s.session_id == session_db_id);
    if !same_links {
        return Ok(false);
    }
    let mut existing: Vec<Key> = stored
        .into_iter()
        .map(|s| {
            (
                s.kind,
                s.ord,
                s.reason,
                s.sent_patch_start,
                s.sent_patch_end,
                s.sent_patch_bytes,
            )
        })
        .collect();
    existing.sort();
    Ok(existing == keys(rows))
}
