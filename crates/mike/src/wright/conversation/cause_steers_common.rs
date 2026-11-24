use serde_json::{Value, json};

use super::SteerError;
use crate::store::persistence::web_repo::WebPersist as _;
use crate::store::persistence::wright::{CausePatch, CauseRow, PatchOutcome};
use crate::store::persistence::{PersistError, PersistHandle};

const QUEUE_SCAN: i64 = 2_000;

pub(super) fn failed(e: &PersistError) -> SteerError {
    SteerError::Failed(format!("the database refused the steer: {e}"))
}

pub(super) fn not_queued(kind: &str, ids: &[i64]) -> SteerError {
    SteerError::Refused(format!(
        "{kind} names cause {ids:?}, which is not in the open triage queue: no such cause exists, it was merged into another cause, or none of its problems is open"
    ))
}

pub(super) async fn queued(
    persist: &PersistHandle,
    kind: &str,
    id: i64,
) -> Result<CauseRow, SteerError> {
    persist
        .web()
        .wright_cause(id)
        .await
        .map_err(|e| failed(&e))?
        .ok_or_else(|| not_queued(kind, &[id]))
}

pub(super) async fn queue(persist: &PersistHandle) -> Result<Vec<CauseRow>, SteerError> {
    persist
        .web()
        .list_wright_causes(QUEUE_SCAN)
        .await
        .map_err(|e| failed(&e))
}

pub(super) async fn queued_all(
    persist: &PersistHandle,
    kind: &str,
    ids: &[i64],
) -> Result<Vec<CauseRow>, SteerError> {
    let rows = queue(persist).await?;
    let missing: Vec<i64> = ids
        .iter()
        .copied()
        .filter(|id| !rows.iter().any(|row| row.cause_id == *id))
        .collect();
    if !missing.is_empty() {
        return Err(not_queued(kind, &missing));
    }
    Ok(rows
        .into_iter()
        .filter(|row| ids.contains(&row.cause_id))
        .collect())
}

pub(super) fn state(row: &CauseRow) -> Value {
    json!({
        "cause_id": row.cause_id,
        "check_name": row.check_name,
        "pinned": row.pinned,
        "operator_priority": row.operator_priority,
        "group_id": row.group_id,
    })
}

pub(super) async fn patch(
    persist: &PersistHandle,
    kind: &str,
    id: i64,
    patch: &CausePatch,
) -> Result<CauseRow, SteerError> {
    match persist
        .web()
        .patch_wright_cause(id, patch)
        .await
        .map_err(|e| failed(&e))?
    {
        PatchOutcome::Updated(row) => Ok(*row),
        PatchOutcome::NoCause => Err(not_queued(kind, &[id])),
        PatchOutcome::NoGroup => Err(SteerError::Refused(format!(
            "{kind} names a group that no longer exists"
        ))),
    }
}

pub(super) fn cause_id_of(value: &Value) -> Result<i64, SteerError> {
    value
        .get("cause_id")
        .and_then(Value::as_i64)
        .ok_or_else(|| SteerError::Failed(format!("the stored state has no cause_id: {value}")))
}

pub(super) fn field_patch(field: &str, prior: &Value) -> Result<CausePatch, SteerError> {
    let stored = prior.get(field).ok_or_else(|| {
        SteerError::Failed(format!("the stored prior state has no {field}: {prior}"))
    })?;
    let mut patch = CausePatch::default();
    match field {
        "pinned" => patch.pinned = Some(stored.as_bool().unwrap_or(false)),
        "operator_priority" => patch.operator_priority = Some(stored.as_f64()),
        _ => patch.group_id = Some(stored.as_i64()),
    }
    Ok(patch)
}

pub(super) fn drifted(
    kind: &str,
    field: &str,
    id: i64,
    effect: &Value,
    row: &CauseRow,
) -> Option<SteerError> {
    let current = state(row);
    (current.get(field) != effect.get(field)).then(|| {
        SteerError::Conflict(format!(
            "{kind} left cause {id} with {field} {}, and it is now {}, so it is left as it is",
            effect.get(field).unwrap_or(&Value::Null),
            current.get(field).unwrap_or(&Value::Null)
        ))
    })
}
