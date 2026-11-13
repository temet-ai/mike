use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::Deserialize;
use serde_json::{Map, Value, json};

use super::definition_view::DefinitionError;
use crate::launch::fleet::{
    FleetDefinition, Missing, check_definition, check_environment, gather, merge, starts,
};
use crate::launch::{BackendId, LaunchError, TerminalBackend, backend_for};
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::FleetRow;

pub const STATE_DRAFT: &str = "draft";
pub const STATE_READY: &str = "ready";
pub const STATE_STARTING: &str = "starting";
pub const STATE_STOPPING: &str = "stopping";
pub const STATE_FAILED: &str = "failed";

#[derive(Debug, Deserialize)]
pub struct PatchFleetRequest {
    #[serde(default)]
    pub patch: Option<Value>,
    #[serde(default)]
    pub ready: bool,
}

pub struct Saved {
    pub row: FleetRow,
    pub missing: Vec<Missing>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Move {
    Stay,
    MarkReady,
    BackToDraft,
}

pub fn next_move(state: &str, ready_asked: bool, missing: &[Missing]) -> Move {
    match (state, missing.is_empty()) {
        (STATE_DRAFT, true) if ready_asked => Move::MarkReady,
        (STATE_READY, false) => Move::BackToDraft,
        (STATE_FAILED, true) if ready_asked => Move::MarkReady,
        (STATE_FAILED, _) => Move::BackToDraft,
        _ => Move::Stay,
    }
}

pub fn merged(
    stored: &Value,
    patch: Option<&Value>,
) -> Result<(Value, FleetDefinition), DefinitionError> {
    merged_done(stored, patch, &BTreeSet::new())
}

pub fn merged_done(
    stored: &Value,
    patch: Option<&Value>,
    done: &BTreeSet<String>,
) -> Result<(Value, FleetDefinition), DefinitionError> {
    let mut definition = stored.clone();
    if let Some(patch) = patch {
        if !patch.is_object() {
            return Err(DefinitionError::InvalidDefinition(format!(
                "`patch` must be an object of items to set, with null to remove one; got {patch}"
            )));
        }
        merge(&mut definition, patch);
    }
    match FleetDefinition::from_value(&definition) {
        Ok(parsed) => {
            check_changed_overlaps(stored, &parsed, done)?;
            Ok((definition, parsed))
        }
        Err(problem) => Err(DefinitionError::InvalidDefinition(shape_message(
            &definition,
            &problem,
        ))),
    }
}

fn check_changed_overlaps(
    stored: &Value,
    parsed: &FleetDefinition,
    done: &BTreeSet<String>,
) -> Result<(), DefinitionError> {
    let previous =
        FleetDefinition::from_value(stored).map_err(DefinitionError::InvalidDefinition)?;
    let conflicts =
        crate::launch::fleet::checks_paths::changed_overlaps_done(&previous, parsed, done);
    if conflicts.is_empty() {
        return Ok(());
    }
    Err(DefinitionError::InvalidDefinition(
        conflicts
            .iter()
            .map(|conflict| format!("{}: {}", conflict.item, conflict.problem))
            .collect::<Vec<_>>()
            .join("; "),
    ))
}

pub fn parse_new(definition: Option<Value>) -> Result<(Value, FleetDefinition), DefinitionError> {
    merged(&json!({"briefs": {"as_notes": true}}), definition.as_ref())
}

fn shape_message(definition: &Value, problem: &str) -> String {
    let mut path = Vec::new();
    let leaf = narrow(&mut path, definition);
    if path.is_empty() {
        return problem.to_string();
    }
    let detail = parse_at(&path, leaf).err().unwrap_or_default();
    format!(
        "the item `{}` does not have the expected shape: {detail}",
        path.join(".")
    )
}

fn narrow<'a>(path: &mut Vec<&'a str>, value: &'a Value) -> &'a Value {
    let Value::Object(fields) = value else {
        return value;
    };
    for (key, child) in fields {
        path.push(key);
        if parse_at(path, child).is_err() {
            return narrow(path, child);
        }
        path.pop();
    }
    value
}

fn parse_at(path: &[&str], leaf: &Value) -> Result<(), String> {
    let wrapped = path.iter().rev().fold(leaf.clone(), |inner, key| {
        let mut fields = Map::new();
        fields.insert((*key).to_string(), inner);
        Value::Object(fields)
    });
    serde_json::from_value::<FleetDefinition>(wrapped)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn home_dir() -> PathBuf {
    crate::config::paths::live_install_guard::home().unwrap_or_else(|| PathBuf::from("/"))
}

fn resolve_backend(state: &AppState) -> Result<Box<dyn TerminalBackend>, LaunchError> {
    let id: BackendId = state.config.launch.backend.parse()?;
    backend_for(id).ok_or(LaunchError::BackendNone)
}

pub async fn environment_problems(
    state: &AppState,
    this: uuid::Uuid,
    def: &FleetDefinition,
) -> Vec<Missing> {
    let backend = match resolve_backend(state) {
        Ok(backend) => backend,
        Err(problem) => return vec![Missing::new("terminal", problem.to_string())],
    };
    let limit = usize::try_from(state.config.launch.max_surfaces).unwrap_or(usize::MAX);
    let mut environment = gather(def, &home_dir(), limit, backend.as_ref()).await;
    environment.reserved_surfaces = seats_other_fleets_are_opening(state, this).await;
    check_environment(def, &environment)
}

async fn seats_other_fleets_are_opening(state: &AppState, this: uuid::Uuid) -> usize {
    let Some(persist) = state.persist() else {
        return 0;
    };
    let Ok(rows) = persist
        .fleet()
        .list_fleets_in_states(&[STATE_STARTING])
        .await
    else {
        return 0;
    };
    let mut reserved = 0;
    for row in rows.iter().filter(|row| row.fleet_uuid != this) {
        reserved += still_to_open(persist, row).await;
    }
    reserved
}

async fn still_to_open(persist: &PersistHandle, row: &FleetRow) -> usize {
    let Ok(def) = FleetDefinition::from_value(&row.definition) else {
        return 0;
    };
    let opened = match &row.run_id {
        Some(run) => persist
            .fleet()
            .fleet_seat_rows(run)
            .await
            .map(|seats| seats.iter().filter(|s| s.closed_at.is_none()).count())
            .unwrap_or(0),
        None => 0,
    };
    starts(&def).most_at_once.saturating_sub(opened)
}

async fn save_once(
    state: &AppState,
    persist: &PersistHandle,
    row: &FleetRow,
    body: &PatchFleetRequest,
) -> Result<Option<Saved>, DefinitionError> {
    match row.state.as_str() {
        STATE_STARTING => return Err(DefinitionError::Starting(row.slug.clone())),
        STATE_STOPPING => return Err(DefinitionError::Stopping(row.slug.clone())),
        _ => {}
    }
    let done = match FleetDefinition::from_value(&row.definition) {
        Ok(stored) => {
            crate::launch::fleet::done_tasks_of(persist, row.run_id.as_deref(), &stored).await
        }
        Err(_) => BTreeSet::new(),
    };
    let (definition, parsed) = merged_done(&row.definition, body.patch.as_ref(), &done)?;
    let mut missing = check_definition(&parsed);
    if body.ready {
        missing.extend(environment_problems(state, row.fleet_uuid, &parsed).await);
        missing.extend(crate::launch::fleet::nextest_filter::done_when_filters(&parsed).await);
    }
    let missing_value = json!(missing);
    let saved = persist
        .fleet()
        .save_fleet_definition(row.fleet_uuid, &definition, &missing_value, row.updated_at)
        .await?;
    Ok(saved.map(|updated_at| Saved {
        row: row.saved(definition, missing_value, updated_at),
        missing,
    }))
}

pub async fn save_definition(
    state: &AppState,
    persist: &PersistHandle,
    row: FleetRow,
    body: &PatchFleetRequest,
) -> Result<Saved, DefinitionError> {
    if let Some(saved) = save_once(state, persist, &row, body).await? {
        return Ok(saved);
    }
    let fresh = persist
        .fleet()
        .find_fleet(&row.fleet_uuid.to_string())
        .await?
        .ok_or_else(|| DefinitionError::NotFound(row.slug.clone()))?;
    save_once(state, persist, &fresh, body)
        .await?
        .ok_or(DefinitionError::Changed(fresh.slug))
}
