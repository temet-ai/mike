use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::runtime_repo::RuntimePersist as _;
use serde::Serialize;
use serde_json::Value;

use super::definition_view::DefinitionError;
use crate::launch::fleet::status::FleetStatus;
use crate::launch::fleet::{FleetDefinition, capacity, staff_limits};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet::FleetRow;

pub async fn with_capacity(
    persist: &PersistHandle,
    row: &FleetRow,
    status: &FleetStatus,
    mut value: Value,
) -> Value {
    let (Some(run), Ok(def)) = (
        row.run_id.as_deref(),
        FleetDefinition::from_value(&row.definition),
    ) else {
        return value;
    };
    let seats_live = staff_limits::live_worker_seats(status);
    let capacity = capacity::gather(persist, &def, run, seats_live).await;
    if let Ok(capacity) = serde_json::to_value(capacity) {
        value["capacity"] = capacity;
    }
    value
}

pub async fn with_observations(
    persist: &PersistHandle,
    run: Option<&str>,
    value: &impl Serialize,
) -> Result<Value, DefinitionError> {
    let sessions = match run {
        Some(run) => persist
            .fleet()
            .fleet_seat_rows(run)
            .await?
            .into_iter()
            .filter_map(|row| row.session_uuid)
            .collect::<Vec<_>>(),
        None => Vec::new(),
    };
    let observations = persist.runtime().session_observations(&sessions).await?;
    let mut value =
        serde_json::to_value(value).map_err(|error| DefinitionError::Status(error.to_string()))?;
    value["observations"] = serde_json::to_value(observations)
        .map_err(|error| DefinitionError::Status(error.to_string()))?;
    Ok(value)
}
