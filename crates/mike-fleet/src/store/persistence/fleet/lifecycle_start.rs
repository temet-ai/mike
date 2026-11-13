use super::FleetRow;
use super::lifecycle::{lock_fleet, same_lifecycle, write_state};
use crate::store::persistence::PersistError;
use crate::store::persistence::launch::NewRun;

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn initialize_fleet_run(
        self,
        expected: &FleetRow,
        run: &NewRun,
    ) -> Result<Option<FleetRow>, PersistError> {
        crate::metrics::time_db("launch_initialize_fleet_run", async move {
            let mut tx = self.pool.begin().await?;
            let Some(current) = lock_fleet(&mut tx, expected.fleet_uuid).await? else {
                tx.rollback().await?;
                return Ok(None);
            };
            if !same_lifecycle(&current, expected) || current.state != "starting" {
                tx.rollback().await?;
                return Ok(None);
            }
            if current.run_id.as_deref().is_some_and(|id| id != run.run_id.as_str()) {
                return Err(PersistError::DataInvariant(format!(
                    "fleet {} already owns run {:?}; refusing to rebind to {:?}",
                    current.fleet_uuid, current.run_id, run.run_id
                )));
            }
            crate::store::persistence::launch::in_transaction::upsert_run(&mut tx, run).await?;
            let other_owner: bool = sqlx::query_scalar(
                "SELECT EXISTS (SELECT 1 FROM launch.fleets WHERE run_id = $1 AND fleet_uuid <> $2)",
            )
            .bind(&run.run_id)
            .bind(current.fleet_uuid)
            .fetch_one(&mut *tx)
            .await?;
            if other_owner {
                return Err(PersistError::DataInvariant(format!(
                    "run {:?} already belongs to another fleet; initialization was rolled back",
                    run.run_id
                )));
            }
            let row = if current.run_id.as_deref() == Some(run.run_id.as_str()) {
                current
            } else {
                write_state(&mut tx, current.fleet_uuid, "starting", Some(run.run_id.as_str()), None).await?
            };
            tx.commit().await?;
            self.fleets_changed();
            Ok(Some(row))
        })
        .await
    }
}
