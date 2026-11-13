use serde_json::Value;
use sqlx::PgConnection;
use uuid::Uuid;

use super::FleetRow;
use super::lifecycle_completion::run_settled;
use crate::store::persistence::PersistError;

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn set_fleet_state_guarded(
        self,
        fleet_uuid: Uuid,
        state: &str,
        run_id: Option<&str>,
        start_report: Option<&Value>,
    ) -> Result<FleetRow, PersistError> {
        let mut tx = self.pool.begin().await?;
        let current = lock_fleet(&mut tx, fleet_uuid).await?.ok_or_else(|| {
            PersistError::DataInvariant(format!("fleet {fleet_uuid} does not exist"))
        })?;
        validate_transition(&current, state, run_id, start_report)?;
        validate_retry(&mut tx, &current, state).await?;
        let row = if unchanged(&current, state, run_id, start_report) {
            current
        } else {
            write_state(&mut tx, fleet_uuid, state, run_id, start_report).await?
        };
        tx.commit().await?;
        self.fleets_changed();
        Ok(row)
    }

    pub async fn transition_fleet_state(
        self,
        expected: &FleetRow,
        state: &str,
        run_id: Option<&str>,
        start_report: Option<&Value>,
    ) -> Result<Option<FleetRow>, PersistError> {
        crate::metrics::time_db("launch_transition_fleet_state", async move {
            let mut tx = self.pool.begin().await?;
            let Some(current) = lock_fleet(&mut tx, expected.fleet_uuid).await? else {
                tx.rollback().await?;
                return Ok(None);
            };
            if !same_lifecycle(&current, expected) {
                tx.rollback().await?;
                return Ok(None);
            }
            validate_transition(&current, state, run_id, start_report)?;
            validate_retry(&mut tx, &current, state).await?;
            let row = if unchanged(&current, state, run_id, start_report) {
                current
            } else {
                write_state(&mut tx, expected.fleet_uuid, state, run_id, start_report).await?
            };
            tx.commit().await?;
            self.fleets_changed();
            Ok(Some(row))
        })
        .await
    }
}

pub async fn lock_fleet(
    connection: &mut PgConnection,
    fleet_uuid: Uuid,
) -> Result<Option<FleetRow>, PersistError> {
    Ok(sqlx::query_file_as!(
        FleetRow,
        "src/store/persistence/fleet/lifecycle_sql/lock_fleet.sql",
        fleet_uuid,
    )
    .fetch_optional(connection)
    .await?)
}

pub fn same_lifecycle(current: &FleetRow, expected: &FleetRow) -> bool {
    current.fleet_uuid == expected.fleet_uuid
        && current.state == expected.state
        && current.run_id == expected.run_id
        && current.updated_at == expected.updated_at
}

fn unchanged(
    current: &FleetRow,
    state: &str,
    run_id: Option<&str>,
    start_report: Option<&Value>,
) -> bool {
    current.state == state
        && run_id.is_none_or(|run| current.run_id.as_deref() == Some(run))
        && start_report.is_none_or(|report| current.start_report.as_ref() == Some(report))
}

fn supported_transition(from: &str, to: &str) -> bool {
    match from {
        "draft" => matches!(to, "draft" | "ready"),
        "ready" => matches!(to, "draft" | "ready" | "starting"),
        "starting" => matches!(to, "starting" | "running" | "failed" | "stopping"),
        "running" => matches!(to, "running" | "stopping"),
        "failed" => matches!(to, "failed" | "draft" | "ready" | "stopping"),
        "stopping" => to == "stopping",
        "done" => to == "done",
        _ => false,
    }
}

fn validate_transition(
    current: &FleetRow,
    state: &str,
    run_id: Option<&str>,
    start_report: Option<&Value>,
) -> Result<(), PersistError> {
    let run_changed = match (current.run_id.as_deref(), run_id) {
        (Some(current), Some(next)) => current != next,
        (None, Some(_)) => !matches!(state, "starting" | "running"),
        _ => false,
    };
    let terminal_changed = matches!(current.state.as_str(), "stopping" | "done")
        && !unchanged(current, state, run_id, start_report);
    if !supported_transition(&current.state, state) || run_changed || terminal_changed {
        return Err(PersistError::DataInvariant(format!(
            "fleet {} is {:?} in run {:?}; refusing transition to {state:?} in run {run_id:?}; done requires verified completion",
            current.fleet_uuid, current.state, current.run_id
        )));
    }
    Ok(())
}

pub async fn write_state(
    connection: &mut PgConnection,
    fleet_uuid: Uuid,
    state: &str,
    run_id: Option<&str>,
    start_report: Option<&Value>,
) -> Result<FleetRow, PersistError> {
    Ok(sqlx::query_file_as!(
        FleetRow,
        "src/store/persistence/fleet/set_fleet_state.sql",
        fleet_uuid,
        state,
        run_id,
        start_report,
    )
    .fetch_one(connection)
    .await?)
}

async fn validate_retry(
    connection: &mut PgConnection,
    current: &FleetRow,
    state: &str,
) -> Result<(), PersistError> {
    if current.state == "failed"
        && matches!(state, "draft" | "ready")
        && !run_settled(connection, current.run_id.as_deref()).await?
    {
        return Err(PersistError::DataInvariant(format!(
            "fleet {} has unresolved launches or managed commands; stop and settle run {:?} before retrying",
            current.fleet_uuid, current.run_id
        )));
    }
    Ok(())
}
