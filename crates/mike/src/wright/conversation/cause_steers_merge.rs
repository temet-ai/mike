use serde_json::{Value, json};

use super::SteerError;
use super::cause_steers_common as common;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::web_repo::WebPersist as _;
use crate::store::persistence::wright::{MergeOutcome, SeparateOutcome};

fn ids_of(value: &Value, field: &str) -> Result<Vec<i64>, SteerError> {
    value
        .get(field)
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_i64).collect())
        .ok_or_else(|| SteerError::Failed(format!("the stored state has no {field}: {value}")))
}

fn int_of(value: &Value, field: &str) -> Result<i64, SteerError> {
    value
        .get(field)
        .and_then(Value::as_i64)
        .ok_or_else(|| SteerError::Failed(format!("the stored state has no {field}: {value}")))
}

pub(super) async fn read_merge(
    persist: &PersistHandle,
    into: i64,
    cause_ids: &[i64],
) -> Result<Value, SteerError> {
    common::queued(persist, "merge_causes", into).await?;
    common::queued_all(persist, "merge_causes", cause_ids).await?;
    Ok(json!({"into": into, "cause_ids": cause_ids}))
}

pub(super) async fn apply_merge(
    persist: &PersistHandle,
    into: i64,
    cause_ids: &[i64],
) -> Result<Value, SteerError> {
    let outcome = persist
        .web()
        .merge_wright_causes_by_operator(into, cause_ids)
        .await
        .map_err(|e| common::failed(&e))?;
    match outcome {
        MergeOutcome::Merged(merged) => Ok(json!({
            "into": into,
            "cause_ids": cause_ids,
            "causes": merged.causes,
            "problems": merged.problems,
            "merge_ids": merged.merge_ids,
        })),
        MergeOutcome::NoRoot => Err(common::not_queued("merge_causes", &[into])),
        MergeOutcome::Unknown(unknown) => Err(common::not_queued("merge_causes", &unknown)),
    }
}

pub(super) async fn undo_merge(
    persist: &PersistHandle,
    effect: &Value,
) -> Result<Value, SteerError> {
    let mut separated = Vec::new();
    for merge_id in ids_of(effect, "merge_ids")?.into_iter().rev() {
        let outcome = persist
            .web()
            .separate_wright_cause_merge(merge_id)
            .await
            .map_err(|e| common::failed(&e))?;
        match outcome {
            SeparateOutcome::Separated(_) => separated.push(merge_id),
            SeparateOutcome::AlreadySeparated => {}
            SeparateOutcome::NotFound | SeparateOutcome::EngineerMerge => {
                return Err(SteerError::Conflict(format!(
                    "merge {merge_id} no longer exists as an operator merge, so it is left as it is"
                )));
            }
        }
    }
    Ok(json!({"separated": separated}))
}

pub(super) async fn read_separate(
    persist: &PersistHandle,
    merge_id: i64,
) -> Result<Value, SteerError> {
    let rows = common::queue(persist).await?;
    let found = rows.iter().find_map(|row| {
        row.absorbed
            .0
            .iter()
            .find(|merge| merge.merge_id == merge_id)
            .map(|merge| (row.cause_id, merge))
    });
    let Some((root, merge)) = found else {
        return Err(SteerError::Refused(format!(
            "separate_merge names merge {merge_id}, and no open merge has that id on a cause in the triage queue: it does not exist, it was already separated, or its cause left the queue"
        )));
    };
    if merge.by != "operator" {
        return Err(SteerError::Refused(format!(
            "separate_merge names merge {merge_id}, which the engineer made because a finding named the causes as one fault; only a merge the operator made can be separated"
        )));
    }
    Ok(json!({
        "merge_id": merge_id,
        "root_cause_id": root,
        "absorbed_cause_id": merge.cause_id,
        "problems": merge.problems,
    }))
}

pub(super) async fn apply_separate(
    persist: &PersistHandle,
    merge_id: i64,
) -> Result<Value, SteerError> {
    let outcome = persist
        .web()
        .separate_wright_cause_merge(merge_id)
        .await
        .map_err(|e| common::failed(&e))?;
    match outcome {
        SeparateOutcome::Separated(row) => Ok(json!({
            "merge_id": row.id,
            "root_cause_id": row.root_cause_id,
            "absorbed_cause_id": row.absorbed_cause_id,
            "problem_ids": row.problem_ids,
        })),
        SeparateOutcome::NotFound => Err(SteerError::Refused(format!(
            "separate_merge names merge {merge_id}, and no merge has that id"
        ))),
        SeparateOutcome::EngineerMerge => Err(SteerError::Refused(format!(
            "merge {merge_id} was made by the engineer; only a merge the operator made can be separated"
        ))),
        SeparateOutcome::AlreadySeparated => Err(SteerError::Refused(format!(
            "merge {merge_id} was already separated"
        ))),
    }
}

pub(super) async fn undo_separate(
    persist: &PersistHandle,
    prior: &Value,
) -> Result<Value, SteerError> {
    let root = int_of(prior, "root_cause_id")?;
    let absorbed = int_of(prior, "absorbed_cause_id")?;
    let outcome = persist
        .web()
        .merge_wright_causes_by_operator(root, &[absorbed])
        .await
        .map_err(|e| common::failed(&e))?;
    match outcome {
        MergeOutcome::Merged(merged) => Ok(json!({"merge_ids": merged.merge_ids})),
        MergeOutcome::NoRoot => Err(SteerError::Conflict(format!(
            "cause {root} no longer exists or was merged into another cause, so cause {absorbed} stays separate"
        ))),
        MergeOutcome::Unknown(_) => Err(SteerError::Conflict(format!(
            "cause {absorbed} no longer exists or was merged into another cause since the separation, so it is left as it is"
        ))),
    }
}
