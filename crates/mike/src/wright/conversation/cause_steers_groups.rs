use serde_json::{Value, json};

use super::SteerError;
use super::cause_steers_common as common;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::web_repo::WebPersist as _;
use crate::store::persistence::wright::{CausePatch, CauseRow, GroupWrite};

const GROUPING: &str = "group_causes";
const UNGROUPING: &str = "ungroup_causes";

struct Known {
    id: i64,
    name: String,
    causes: i64,
}

async fn known_groups(persist: &PersistHandle) -> Result<Vec<Known>, SteerError> {
    let groups = persist
        .web()
        .list_wright_cause_groups()
        .await
        .map_err(|e| common::failed(&e))?;
    Ok(groups
        .into_iter()
        .map(|group| Known {
            id: group.id,
            name: group.name,
            causes: group.causes,
        })
        .collect())
}

fn find_named<'a>(groups: &'a [Known], name: &str) -> Option<&'a Known> {
    let wanted = name.to_lowercase();
    groups
        .iter()
        .find(|group| group.name.to_lowercase() == wanted)
}

fn membership(rows: &[CauseRow]) -> Value {
    Value::Array(
        rows.iter()
            .map(|row| json!({"cause_id": row.cause_id, "group_id": row.group_id}))
            .collect(),
    )
}

fn prior_memberships(prior: &Value) -> Result<Vec<(i64, Option<i64>)>, SteerError> {
    let items = prior
        .get("causes")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            SteerError::Failed(format!("the stored prior state has no causes: {prior}"))
        })?;
    items
        .iter()
        .map(|item| {
            Ok((
                common::cause_id_of(item)?,
                item.get("group_id").and_then(Value::as_i64),
            ))
        })
        .collect()
}

fn effect_ids(effect: &Value) -> Result<Vec<i64>, SteerError> {
    effect
        .get("cause_ids")
        .and_then(Value::as_array)
        .map(|items| items.iter().filter_map(Value::as_i64).collect())
        .ok_or_else(|| SteerError::Failed(format!("the stored effect has no cause_ids: {effect}")))
}

fn group_patch(group_id: Option<i64>) -> CausePatch {
    CausePatch {
        group_id: Some(group_id),
        ..CausePatch::default()
    }
}

pub(super) async fn read_group(
    persist: &PersistHandle,
    name: &str,
    cause_ids: &[i64],
) -> Result<Value, SteerError> {
    let rows = common::queued_all(persist, GROUPING, cause_ids).await?;
    let groups = known_groups(persist).await?;
    Ok(json!({
        "name": name,
        "group_id": find_named(&groups, name).map(|group| group.id),
        "causes": membership(&rows),
    }))
}

async fn ensure_group(persist: &PersistHandle, name: &str) -> Result<(i64, bool), SteerError> {
    let groups = known_groups(persist).await?;
    if let Some(group) = find_named(&groups, name) {
        return Ok((group.id, false));
    }
    let written = persist
        .web()
        .create_wright_cause_group(name)
        .await
        .map_err(|e| common::failed(&e))?;
    match written {
        GroupWrite::Saved(group) => Ok((group.id, true)),
        GroupWrite::NameTaken | GroupWrite::NoGroup => {
            let groups = known_groups(persist).await?;
            find_named(&groups, name)
                .map(|group| (group.id, false))
                .ok_or_else(|| {
                    SteerError::Failed(format!("the group `{name}` could not be created or found"))
                })
        }
    }
}

async fn delete_if_empty(persist: &PersistHandle, group_id: i64) -> Result<bool, SteerError> {
    let groups = known_groups(persist).await?;
    let empty = groups
        .iter()
        .any(|group| group.id == group_id && group.causes == 0);
    if !empty {
        return Ok(false);
    }
    persist
        .web()
        .delete_wright_cause_group(group_id)
        .await
        .map_err(|e| common::failed(&e))
}

async fn put_back(persist: &PersistHandle, kind: &str, rows: &[&CauseRow]) {
    for row in rows {
        if let Err(e) = common::patch(persist, kind, row.cause_id, &group_patch(row.group_id)).await
        {
            tracing::warn!(cause = row.cause_id, reason = %e, "wright: a group change could not be put back");
        }
    }
}

pub(super) async fn apply_group(
    persist: &PersistHandle,
    name: &str,
    cause_ids: &[i64],
) -> Result<Value, SteerError> {
    let rows = common::queued_all(persist, GROUPING, cause_ids).await?;
    let (group_id, created) = ensure_group(persist, name).await?;
    let mut changed: Vec<&CauseRow> = Vec::new();
    for row in &rows {
        match common::patch(
            persist,
            GROUPING,
            row.cause_id,
            &group_patch(Some(group_id)),
        )
        .await
        {
            Ok(_) => changed.push(row),
            Err(e) => {
                put_back(persist, GROUPING, &changed).await;
                if created {
                    delete_if_empty(persist, group_id).await.ok();
                }
                return Err(e);
            }
        }
    }
    Ok(json!({
        "name": name,
        "group_id": group_id,
        "group_created": created,
        "cause_ids": cause_ids,
    }))
}

async fn restore_all(
    persist: &PersistHandle,
    kind: &str,
    restores: &[(i64, Option<i64>)],
) -> Result<(), SteerError> {
    let live: Vec<i64> = known_groups(persist)
        .await?
        .into_iter()
        .map(|group| group.id)
        .collect();
    for (cause_id, group_id) in restores {
        let target = group_id.filter(|id| live.contains(id));
        common::patch(persist, kind, *cause_id, &group_patch(target)).await?;
    }
    Ok(())
}

pub(super) async fn undo_group(
    persist: &PersistHandle,
    prior: &Value,
    effect: &Value,
) -> Result<Value, SteerError> {
    let group_id = effect
        .get("group_id")
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            SteerError::Failed(format!("the stored effect has no group_id: {effect}"))
        })?;
    let queue = common::queue(persist).await?;
    let ids = effect_ids(effect)?;
    let present: Vec<&CauseRow> = queue
        .iter()
        .filter(|row| ids.contains(&row.cause_id))
        .collect();
    if let Some(moved) = present.iter().find(|row| row.group_id != Some(group_id)) {
        return Err(SteerError::Conflict(format!(
            "cause {} is no longer in the group the steer put it in (it is in group {:?} now), so the grouping is left as it is",
            moved.cause_id, moved.group_id
        )));
    }
    let before = prior_memberships(prior)?;
    let restores: Vec<(i64, Option<i64>)> = before
        .into_iter()
        .filter(|(id, _)| present.iter().any(|row| row.cause_id == *id))
        .collect();
    restore_all(persist, GROUPING, &restores).await?;
    let created = effect.get("group_created").and_then(Value::as_bool) == Some(true);
    let deleted = created && delete_if_empty(persist, group_id).await?;
    Ok(json!({"restored": restores.len(), "group_deleted": deleted}))
}

pub(super) async fn read_ungroup(
    persist: &PersistHandle,
    cause_ids: &[i64],
) -> Result<Value, SteerError> {
    let rows = common::queued_all(persist, UNGROUPING, cause_ids).await?;
    Ok(json!({"causes": membership(&rows)}))
}

pub(super) async fn apply_ungroup(
    persist: &PersistHandle,
    cause_ids: &[i64],
) -> Result<Value, SteerError> {
    let rows = common::queued_all(persist, UNGROUPING, cause_ids).await?;
    let mut cleared = Vec::new();
    for row in rows.iter().filter(|row| row.group_id.is_some()) {
        common::patch(persist, UNGROUPING, row.cause_id, &group_patch(None)).await?;
        cleared.push(row.cause_id);
    }
    Ok(json!({"cause_ids": cleared}))
}

pub(super) async fn undo_ungroup(
    persist: &PersistHandle,
    prior: &Value,
    effect: &Value,
) -> Result<Value, SteerError> {
    let queue = common::queue(persist).await?;
    let cleared = effect_ids(effect)?;
    let present: Vec<&CauseRow> = queue
        .iter()
        .filter(|row| cleared.contains(&row.cause_id))
        .collect();
    if let Some(regrouped) = present.iter().find(|row| row.group_id.is_some()) {
        return Err(SteerError::Conflict(format!(
            "cause {} was put in group {:?} after the steer took it out, so it is left as it is",
            regrouped.cause_id, regrouped.group_id
        )));
    }
    let restores: Vec<(i64, Option<i64>)> = prior_memberships(prior)?
        .into_iter()
        .filter(|(id, _)| present.iter().any(|row| row.cause_id == *id))
        .collect();
    let live: Vec<i64> = known_groups(persist)
        .await?
        .into_iter()
        .map(|group| group.id)
        .collect();
    if let Some((id, Some(gone))) = restores
        .iter()
        .find(|(_, group)| group.is_some_and(|g| !live.contains(&g)))
    {
        return Err(SteerError::Conflict(format!(
            "group {gone} no longer exists, so cause {id} stays without a group"
        )));
    }
    restore_all(persist, UNGROUPING, &restores).await?;
    Ok(json!({"restored": restores.len()}))
}
