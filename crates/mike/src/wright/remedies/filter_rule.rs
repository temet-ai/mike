use crate::store::persistence::proxy_repo::ProxyPersist as _;
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

use super::{Halt, Owners};
use crate::filter::FilterRule;
use crate::filter::service::{FilterRulePatchInput, FilterServiceError};
use crate::store::persistence::wright::remedies::{ActionRow, PendingAction};

fn rule_id(resource: &str, problem_id: i64) -> Result<i64, Halt> {
    resource.trim().parse().map_err(|_| {
        Halt::Refused(format!(
            "problem {problem_id} names resource `{resource}`, which is not a filter rule id"
        ))
    })
}

async fn read(owners: &Owners, id: i64) -> Result<FilterRule, Halt> {
    owners
        .persist
        .proxy()
        .list_filter_rules()
        .await?
        .into_iter()
        .find(|rule| rule.id == id)
        .ok_or_else(|| Halt::Refused(format!("no live filter rule has id {id}")))
}

fn owner_error(e: FilterServiceError) -> Halt {
    match e {
        FilterServiceError::Persistence(e) => e.into(),
        other => Halt::Refused(other.to_string()),
    }
}

async fn set_enabled(owners: &Owners, id: i64, enabled: bool) -> Result<(), Halt> {
    let patch = FilterRulePatchInput {
        enabled: Some(enabled),
        ..FilterRulePatchInput::default()
    };
    owners
        .filters()?
        .update(id, patch)
        .await
        .map(|_| ())
        .map_err(owner_error)
}

pub(crate) async fn prior(owners: &Owners, item: &PendingAction) -> Result<Value, Halt> {
    let rule = read(owners, rule_id(&item.resource, item.problem_id)?).await?;
    if !rule.enabled {
        return Err(Halt::Refused(format!(
            "filter rule {} is already disabled",
            rule.id
        )));
    }
    Ok(json!({"rule_id": rule.id, "enabled": true, "updated_at": rule.updated_at}))
}

pub(crate) async fn apply(owners: &Owners, item: &PendingAction) -> Result<Value, Halt> {
    let id = rule_id(&item.resource, item.problem_id)?;
    owners.filters()?;
    set_enabled(owners, id, false).await?;
    Ok(json!({"outcome": "disabled", "rule_id": id}))
}

pub(crate) async fn verify(owners: &Owners, item: &PendingAction) -> Result<Value, Halt> {
    let rule = read(owners, rule_id(&item.resource, item.problem_id)?).await?;
    if rule.enabled {
        return Err(Halt::Failed(format!(
            "filter rule {} is still enabled when read again",
            rule.id
        )));
    }
    Ok(
        json!({"outcome": "disabled", "rule_id": rule.id, "enabled": false, "updated_at": rule.updated_at}),
    )
}

fn stamped(effect: &Value) -> Option<DateTime<Utc>> {
    serde_json::from_value(effect["updated_at"].clone()).ok()
}

pub(crate) async fn undo(owners: &Owners, row: &ActionRow) -> Result<Value, Halt> {
    let effect = row.effect.clone().unwrap_or(Value::Null);
    let id = effect["rule_id"]
        .as_i64()
        .ok_or_else(|| Halt::Refused(format!("action {} recorded no rule id", row.id)))?;
    owners.filters()?;
    let rule = read(owners, id).await?;
    if rule.enabled || Some(rule.updated_at) != stamped(&effect) {
        return Err(Halt::Refused(format!(
            "filter rule {id} changed since Wright disabled it (enabled {}, updated {})",
            rule.enabled, rule.updated_at
        )));
    }
    set_enabled(owners, id, true).await?;
    let after = read(owners, id).await?;
    if !after.enabled {
        return Err(Halt::Transient(format!(
            "filter rule {id} is still disabled after the undo"
        )));
    }
    Ok(json!({"rule_id": id, "enabled": true, "updated_at": after.updated_at}))
}
