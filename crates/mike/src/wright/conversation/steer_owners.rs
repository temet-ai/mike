use crate::store::persistence::web_repo::WebPersist as _;
use serde_json::{Value, json};

use super::SteerError;
use super::steer::LimitField;
use crate::store::persistence::wright::{EngineerLimits, LimitsPatch, RemedyPatch, RemedyRow};
use crate::store::persistence::{PersistError, PersistHandle};

fn failed(e: &PersistError) -> SteerError {
    SteerError::Failed(format!("the database refused the steer: {e}"))
}

fn remedy_state(row: &RemedyRow) -> Value {
    json!({
        "name": row.name,
        "enabled": row.enabled,
        "paused_at": row.paused_at,
        "pause_reason": row.pause_reason,
    })
}

pub(crate) async fn read_remedy(persist: &PersistHandle, name: &str) -> Result<Value, SteerError> {
    let rows = persist
        .web()
        .wright_remedies(Some(name))
        .await
        .map_err(|e| failed(&e))?;
    rows.first()
        .map(remedy_state)
        .ok_or_else(|| SteerError::Refused(format!("Wright has no known fix named `{name}`")))
}

pub(crate) async fn set_remedy_enabled(
    persist: &PersistHandle,
    name: &str,
    enabled: bool,
) -> Result<Value, SteerError> {
    let patch = RemedyPatch {
        enabled: Some(enabled),
        daily_limit: None,
        max_resources: None,
    };
    persist
        .web()
        .patch_wright_remedy(name, &patch)
        .await
        .map_err(|e| failed(&e))?
        .as_ref()
        .map(remedy_state)
        .ok_or_else(|| SteerError::Refused(format!("Wright has no known fix named `{name}`")))
}

pub(crate) async fn undo_remedy(
    persist: &PersistHandle,
    prior: &Value,
    effect: &Value,
) -> Result<Value, SteerError> {
    let name = effect
        .get("name")
        .and_then(Value::as_str)
        .ok_or_else(|| SteerError::Failed(format!("the stored effect names no fix: {effect}")))?;
    let current = read_remedy(persist, name).await?;
    if current.get("enabled") != effect.get("enabled") {
        return Err(SteerError::Conflict(format!(
            "known fix `{name}` changed after the steer: it was {effect}, it is now {current}"
        )));
    }
    let enabled = prior
        .get("enabled")
        .and_then(Value::as_bool)
        .unwrap_or(true);
    set_remedy_enabled(persist, name, enabled).await
}

fn limits_state(limits: &EngineerLimits) -> Value {
    json!({
        "daily_starts": limits.daily_starts,
        "wall_seconds": limits.wall_seconds,
        "tool_calls": limits.tool_calls,
        "tokens": limits.tokens,
    })
}

pub(crate) async fn read_limits(persist: &PersistHandle) -> Result<Value, SteerError> {
    persist
        .web()
        .wright_engineer_limits()
        .await
        .map(|limits| limits_state(&limits))
        .map_err(|e| failed(&e))
}

fn limit_patch(field: LimitField, value: i64) -> Result<LimitsPatch, SteerError> {
    let small = || {
        i32::try_from(value)
            .map_err(|_| SteerError::Refused(format!("{} {value} is out of range", field.code())))
    };
    let mut patch = LimitsPatch::default();
    match field {
        LimitField::DailyStarts => patch.daily_starts = Some(small()?),
        LimitField::WallSeconds => patch.wall_seconds = Some(small()?),
        LimitField::ToolCalls => patch.tool_calls = Some(small()?),
        LimitField::Tokens => patch.tokens = Some(value),
    }
    Ok(patch)
}

pub(crate) async fn set_limit(
    persist: &PersistHandle,
    field: LimitField,
    value: i64,
) -> Result<Value, SteerError> {
    persist
        .web()
        .patch_wright_engineer_limits(limit_patch(field, value)?)
        .await
        .map(|limits| limits_state(&limits))
        .map_err(|e| failed(&e))
}

pub(crate) async fn undo_limit(
    persist: &PersistHandle,
    field: LimitField,
    prior: &Value,
    effect: &Value,
) -> Result<Value, SteerError> {
    let current = read_limits(persist).await?;
    let key = field.code();
    if current.get(key) != effect.get(key) {
        return Err(SteerError::Conflict(format!(
            "the engineer's {key} changed after the steer: it was {}, it is now {}",
            effect.get(key).unwrap_or(&Value::Null),
            current.get(key).unwrap_or(&Value::Null)
        )));
    }
    let before = prior.get(key).and_then(Value::as_i64).ok_or_else(|| {
        SteerError::Failed(format!("the stored prior state has no {key}: {prior}"))
    })?;
    set_limit(persist, field, before).await
}
