use crate::store::persistence::fleet_repo::FleetPersist as _;
use uuid::Uuid;

use crate::store::persistence::fleet::FleetRow;
use crate::store::persistence::{PersistError, PersistHandle};

pub async fn seed_fleet_starting(
    handle: &PersistHandle,
    fleet: Uuid,
    run_id: Option<&str>,
) -> Result<FleetRow, PersistError> {
    let row = test_fleet_row(handle, fleet).await?;
    let ready = required(
        handle
            .fleet()
            .transition_fleet_state(&row, "ready", None, None)
            .await?,
        fleet,
        "ready",
    )?;
    required(
        handle
            .fleet()
            .transition_fleet_state(&ready, "starting", run_id, None)
            .await?,
        fleet,
        "starting",
    )
}

pub async fn seed_fleet_running(
    handle: &PersistHandle,
    fleet: Uuid,
    run_id: &str,
) -> Result<FleetRow, PersistError> {
    let starting = seed_fleet_starting(handle, fleet, Some(run_id)).await?;
    required(
        handle
            .fleet()
            .transition_fleet_state(&starting, "running", None, None)
            .await?,
        fleet,
        "running",
    )
}

pub async fn seed_fleet_done(
    handle: &PersistHandle,
    fleet: Uuid,
) -> Result<FleetRow, PersistError> {
    let row = test_fleet_row(handle, fleet).await?;
    let stopping = required(
        handle
            .fleet()
            .transition_fleet_state(&row, "stopping", None, None)
            .await?,
        fleet,
        "stopping",
    )?;
    required(
        handle.fleet().complete_fleet_run(&stopping, None).await?,
        fleet,
        "done",
    )
}

async fn test_fleet_row(handle: &PersistHandle, fleet: Uuid) -> Result<FleetRow, PersistError> {
    handle
        .fleet()
        .find_fleet(&fleet.to_string())
        .await?
        .ok_or_else(|| PersistError::DataInvariant(format!("test fleet {fleet} disappeared")))
}

fn required(row: Option<FleetRow>, fleet: Uuid, state: &str) -> Result<FleetRow, PersistError> {
    row.ok_or_else(|| {
        PersistError::DataInvariant(format!(
            "test fleet {fleet} could not transition to {state}; snapshot changed or work remains unresolved"
        ))
    })
}
