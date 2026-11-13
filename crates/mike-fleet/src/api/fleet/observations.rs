use crate::store::persistence::fleet_repo::FleetPersist as _;
use crate::store::persistence::runtime_repo::RuntimePersist as _;
use serde::Serialize;
use serde_json::Value;

use super::definition_view::DefinitionError;
use crate::store::persistence::PersistHandle;

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
