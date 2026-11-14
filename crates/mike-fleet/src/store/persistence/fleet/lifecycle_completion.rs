use serde_json::Value;
use sqlx::PgConnection;

use super::FleetRow;
use super::lifecycle::{lock_fleet, same_lifecycle};
use crate::store::persistence::PersistError;

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn complete_fleet_run(
        self,
        expected: &FleetRow,
        definition: Option<&Value>,
    ) -> Result<Option<FleetRow>, PersistError> {
        crate::metrics::time_db("launch_complete_fleet_run", async move {
            let mut tx = self.pool.begin().await?;
            let Some(current) = lock_fleet(&mut tx, expected.fleet_uuid).await? else {
                tx.rollback().await?;
                return Ok(None);
            };
            if current.state == "done" && current.run_id == expected.run_id {
                tx.rollback().await?;
                return Ok(None);
            }
            if !same_lifecycle(&current, expected)
                || current.state != "stopping"
                || definition.is_some() && current.updated_at != expected.updated_at
                || !retrospective_staged(&current, definition)?
                || !run_settled(&mut tx, current.run_id.as_deref()).await?
            {
                tx.rollback().await?;
                return Ok(None);
            }
            close_owned_run(&mut tx, current.run_id.as_deref()).await?;
            let row = complete_locked(&mut tx, &current, definition).await?;
            tx.commit().await?;
            self.fleets_changed();
            Ok(Some(row))
        })
        .await
    }
}

pub async fn run_settled(
    connection: &mut PgConnection,
    run_id: Option<&str>,
) -> Result<bool, PersistError> {
    let Some(run_id) = run_id else {
        return Ok(true);
    };
    Ok(sqlx::query_file_scalar!(
        "src/store/persistence/fleet/lifecycle_sql/run_settled.sql",
        run_id,
    )
    .fetch_one(connection)
    .await?)
}

async fn close_owned_run(
    connection: &mut PgConnection,
    run_id: Option<&str>,
) -> Result<(), PersistError> {
    let Some(run_id) = run_id else {
        return Ok(());
    };
    let closed =
        crate::store::persistence::launch::in_transaction::close_run(connection, run_id).await?;
    closed.then_some(()).ok_or_else(|| {
        PersistError::DataInvariant(format!(
            "run {run_id:?} disappeared; fleet completion was rolled back"
        ))
    })
}

async fn complete_locked(
    connection: &mut PgConnection,
    current: &FleetRow,
    definition: Option<&Value>,
) -> Result<FleetRow, PersistError> {
    let row = sqlx::query_file_as!(
        FleetRow,
        "src/store/persistence/fleet/lifecycle_sql/complete_fleet.sql",
        current.fleet_uuid,
        current.state,
        current.run_id.as_deref(),
        definition,
    )
    .fetch_optional(connection)
    .await?;
    row.ok_or_else(|| {
        PersistError::DataInvariant(format!(
            "fleet {} changed under its completion lock; run closure was rolled back",
            current.fleet_uuid
        ))
    })
}

fn retrospective_staged(
    current: &FleetRow,
    definition: Option<&Value>,
) -> Result<bool, PersistError> {
    let Some(run_id) = current.run_id.as_deref() else {
        return Ok(true);
    };
    let definition = definition.unwrap_or(&current.definition);
    let enabled = match definition.pointer("/retrospective/enabled") {
        None | Some(Value::Bool(false)) => false,
        Some(Value::Bool(true)) => true,
        Some(_) => {
            return Err(PersistError::DataInvariant(
                "retrospective.enabled must be boolean before fleet completion".into(),
            ));
        }
    };
    Ok(!enabled
        || definition
            .pointer("/retrospective_job/source_run")
            .and_then(Value::as_str)
            == Some(run_id)
            && definition
                .pointer("/retrospective_job/phase")
                .and_then(Value::as_str)
                == Some("pending"))
}
