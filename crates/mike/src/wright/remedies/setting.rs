use serde_json::{Value, json};

use super::{Halt, Owners};
use crate::change_log::ChangeSource;
use crate::settings::service::SettingsServiceError;
use crate::settings::{SETTINGS_META, SettingMeta, SettingScope};
use crate::store::persistence::change_log::ChangeLogFilter;
use crate::store::persistence::wright::remedies::{ActionRow, PendingAction};

const OWN_PREFIX: &str = "wright";

struct Wanted {
    key: String,
    value: Value,
}

fn wanted(item: &PendingAction) -> Result<Wanted, Halt> {
    let (Some(key), Some(value)) = (item.params["key"].as_str(), item.params.get("value")) else {
        return Err(Halt::Refused(format!(
            "remedy {} needs params with a string `key` and a `value`; it has {}",
            item.remedy, item.params
        )));
    };
    Ok(Wanted {
        key: key.to_owned(),
        value: value.clone(),
    })
}

fn meta(key: &str) -> Result<&'static SettingMeta, Halt> {
    if key.starts_with(OWN_PREFIX) {
        return Err(Halt::Refused(format!(
            "setting `{key}` is one of Wright's own; Wright never changes its own switch, limits or measures"
        )));
    }
    let meta = SETTINGS_META
        .iter()
        .find(|meta| meta.key == key)
        .ok_or_else(|| Halt::Refused(format!("`{key}` is not a known setting")))?;
    if meta.scope == SettingScope::Machine {
        return Err(Halt::Refused(format!(
            "setting `{key}` is set per machine; a known fix changes only settings shared by every machine"
        )));
    }
    Ok(meta)
}

async fn current(owners: &Owners, key: &str) -> Result<Option<Value>, Halt> {
    Ok(owners
        .persist
        .fetch_shared_settings()
        .await?
        .into_iter()
        .find_map(|(k, v)| (k == key).then_some(v)))
}

async fn latest_change(owners: &Owners, key: &str) -> Result<Option<i64>, Halt> {
    let filter = ChangeLogFilter {
        subject_kind: Some("setting".to_owned()),
        subject_key: Some(key.to_owned()),
        limit: 1,
        ..ChangeLogFilter::default()
    };
    Ok(owners
        .persist
        .list_config_changes(&filter)
        .await?
        .first()
        .map(|row| row.id))
}

fn owner_error(e: SettingsServiceError) -> Halt {
    match e {
        SettingsServiceError::Persistence(e) => e.into(),
        other => Halt::Refused(other.to_string()),
    }
}

async fn write(
    owners: &Owners,
    key: &str,
    value: Value,
    source: &ChangeSource,
) -> Result<(Value, Vec<i64>), Halt> {
    owners
        .settings()?
        .set_for_machine_with_source(key, value, None, source)
        .await
        .map_err(owner_error)
}

pub(crate) async fn prior(owners: &Owners, item: &PendingAction) -> Result<Value, Halt> {
    let wanted = wanted(item)?;
    meta(&wanted.key)?;
    let value = current(owners, &wanted.key).await?;
    if value.as_ref() == Some(&wanted.value) {
        return Err(Halt::Refused(format!(
            "setting `{}` is already {}",
            wanted.key, wanted.value
        )));
    }
    Ok(json!({
        "key": wanted.key,
        "present": value.is_some(),
        "value": value,
        "last_change_id": latest_change(owners, &wanted.key).await?,
    }))
}

pub(crate) async fn apply(
    owners: &Owners,
    item: &PendingAction,
    action_id: i64,
) -> Result<Value, Halt> {
    let wanted = wanted(item)?;
    meta(&wanted.key)?;
    let source = ChangeSource::internal().with_detail("wright_action", json!(action_id));
    let (value, change_ids) = write(owners, &wanted.key, wanted.value, &source).await?;
    Ok(json!({"outcome": "set", "key": wanted.key, "value": value, "change_ids": change_ids}))
}

pub(crate) async fn verify(owners: &Owners, effect: &Value) -> Result<Value, Halt> {
    let key = effect["key"].as_str().unwrap_or_default();
    let now = current(owners, key).await?;
    if now.as_ref() != Some(&effect["value"]) {
        return Err(Halt::Failed(format!(
            "setting `{key}` reads {} after it was set to {}",
            now.unwrap_or(Value::Null),
            effect["value"]
        )));
    }
    Ok(effect.clone())
}

fn last_id(effect: &Value) -> Option<i64> {
    effect["change_ids"]
        .as_array()?
        .iter()
        .filter_map(Value::as_i64)
        .max()
}

async fn unchanged_since(owners: &Owners, key: &str, effect: &Value) -> Result<(), Halt> {
    let now = current(owners, key).await?;
    if now.as_ref() != Some(&effect["value"]) {
        return Err(Halt::Refused(format!(
            "setting `{key}` is {} now, not the {} Wright set; it changed since",
            now.unwrap_or(Value::Null),
            effect["value"]
        )));
    }
    let latest = latest_change(owners, key).await?;
    if latest.is_some() && latest != last_id(effect) {
        return Err(Halt::Refused(format!(
            "setting `{key}` has change {} in the change log after Wright's change {}",
            latest.unwrap_or_default(),
            last_id(effect).unwrap_or_default()
        )));
    }
    Ok(())
}

pub(crate) async fn undo(owners: &Owners, row: &ActionRow) -> Result<Value, Halt> {
    let effect = row.effect.clone().unwrap_or(Value::Null);
    let prior = row.prior_state.clone().unwrap_or(Value::Null);
    let key = effect["key"].as_str().unwrap_or_default().to_owned();
    owners.settings()?;
    let meta = meta(&key)?;
    unchanged_since(owners, &key, &effect).await?;
    let back = if prior["present"].as_bool() == Some(true) {
        prior["value"].clone()
    } else {
        meta.default.clone()
    };
    let source = ChangeSource::internal().with_detail("wright_undo", json!(row.id));
    let (value, change_ids) = write(owners, &key, back, &source).await?;
    if current(owners, &key).await?.as_ref() != Some(&value) {
        return Err(Halt::Transient(format!(
            "setting `{key}` does not read {value} after the undo"
        )));
    }
    Ok(json!({"key": key, "value": value, "change_ids": change_ids}))
}
