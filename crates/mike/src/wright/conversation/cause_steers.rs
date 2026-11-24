use serde_json::Value;

use super::cause_steers_common as common;
use super::cause_steers_groups as groups;
use super::cause_steers_merge as merges;
use super::{Change, SteerError};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::wright::CausePatch;

fn single_cause(change: &Change) -> Option<(&'static str, i64)> {
    match change {
        Change::PinCause { cause_id } => Some(("pin_cause", *cause_id)),
        Change::UnpinCause { cause_id } => Some(("unpin_cause", *cause_id)),
        Change::SetCausePriority { cause_id, .. } => Some(("set_cause_priority", *cause_id)),
        _ => None,
    }
}

fn changed_field(change: &Change) -> &'static str {
    match change {
        Change::SetCausePriority { .. } => "operator_priority",
        _ => "pinned",
    }
}

fn patch_for(change: &Change) -> CausePatch {
    match change {
        Change::PinCause { .. } => CausePatch {
            pinned: Some(true),
            ..CausePatch::default()
        },
        Change::UnpinCause { .. } => CausePatch {
            pinned: Some(false),
            ..CausePatch::default()
        },
        Change::SetCausePriority { priority, .. } => CausePatch {
            operator_priority: Some(*priority),
            ..CausePatch::default()
        },
        _ => CausePatch::default(),
    }
}

fn unsupported(change: &Change) -> SteerError {
    SteerError::Failed(format!("{change:?} is not a change to the triage queue"))
}

pub(super) async fn read(persist: &PersistHandle, change: &Change) -> Result<Value, SteerError> {
    if let Some((kind, id)) = single_cause(change) {
        return Ok(common::state(&common::queued(persist, kind, id).await?));
    }
    match change {
        Change::MergeCauses { into, cause_ids } => {
            merges::read_merge(persist, *into, cause_ids).await
        }
        Change::SeparateMerge { merge_id } => merges::read_separate(persist, *merge_id).await,
        Change::GroupCauses { name, cause_ids } => {
            groups::read_group(persist, name, cause_ids).await
        }
        Change::UngroupCauses { cause_ids } => groups::read_ungroup(persist, cause_ids).await,
        other => Err(unsupported(other)),
    }
}

pub(super) async fn apply(persist: &PersistHandle, change: &Change) -> Result<Value, SteerError> {
    if let Some((kind, id)) = single_cause(change) {
        let row = common::patch(persist, kind, id, &patch_for(change)).await?;
        return Ok(common::state(&row));
    }
    match change {
        Change::MergeCauses { into, cause_ids } => {
            merges::apply_merge(persist, *into, cause_ids).await
        }
        Change::SeparateMerge { merge_id } => merges::apply_separate(persist, *merge_id).await,
        Change::GroupCauses { name, cause_ids } => {
            groups::apply_group(persist, name, cause_ids).await
        }
        Change::UngroupCauses { cause_ids } => groups::apply_ungroup(persist, cause_ids).await,
        other => Err(unsupported(other)),
    }
}

async fn undo_single(
    persist: &PersistHandle,
    change: &Change,
    prior: &Value,
    effect: &Value,
) -> Result<Value, SteerError> {
    let Some((kind, id)) = single_cause(change) else {
        return Err(unsupported(change));
    };
    let field = changed_field(change);
    let current = common::queued(persist, kind, id).await?;
    if let Some(conflict) = common::drifted(kind, field, id, effect, &current) {
        return Err(conflict);
    }
    let restore = common::field_patch(field, prior)?;
    let row = common::patch(persist, kind, id, &restore).await?;
    Ok(common::state(&row))
}

pub(super) async fn undo(
    persist: &PersistHandle,
    change: &Change,
    prior: &Value,
    effect: &Value,
) -> Result<Value, SteerError> {
    match change {
        Change::PinCause { .. } | Change::UnpinCause { .. } | Change::SetCausePriority { .. } => {
            undo_single(persist, change, prior, effect).await
        }
        Change::MergeCauses { .. } => merges::undo_merge(persist, effect).await,
        Change::SeparateMerge { .. } => merges::undo_separate(persist, prior).await,
        Change::GroupCauses { .. } => groups::undo_group(persist, prior, effect).await,
        Change::UngroupCauses { .. } => groups::undo_ungroup(persist, prior, effect).await,
        other => Err(unsupported(other)),
    }
}
