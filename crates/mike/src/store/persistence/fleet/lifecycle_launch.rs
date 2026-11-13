use sqlx::{Postgres, Transaction};

use super::lifecycle_rows::FleetRunAdmission;
use crate::store::persistence::PersistError;
use crate::store::persistence::launch::{LaunchSurfaceRefs, NewRun};

pub struct FleetLaunchAdmission {
    transaction: Transaction<'static, Postgres>,
    run_id: String,
}

impl FleetLaunchAdmission {
    pub async fn record_opened(
        mut self,
        launch_id: i64,
        refs: &LaunchSurfaceRefs,
    ) -> Result<(), PersistError> {
        let owned = sqlx::query_scalar!(
            "SELECT id FROM launch.launches WHERE id = $1 AND run_id = $2 FOR UPDATE",
            launch_id,
            self.run_id,
        )
        .fetch_optional(&mut *self.transaction)
        .await?;
        if owned.is_none() {
            return Err(PersistError::DataInvariant(format!(
                "launch {launch_id} does not belong to admitted run {:?}; opening was not recorded",
                self.run_id
            )));
        }
        let recorded = sqlx::query_file_scalar!(
            "src/store/persistence/launch/record_launch_opened.sql",
            launch_id,
            refs.workspace_ref,
            refs.tab_ref,
            refs.surface_ref,
            refs.pid,
            refs.actual_placement,
        )
        .fetch_optional(&mut *self.transaction)
        .await?;
        if recorded.is_none() {
            return Err(PersistError::DataInvariant(format!(
                "launch {launch_id} disappeared while recording its opening"
            )));
        }
        self.record_surface_run(refs).await?;
        self.transaction.commit().await?;
        Ok(())
    }

    async fn record_surface_run(&mut self, refs: &LaunchSurfaceRefs) -> Result<(), PersistError> {
        let updated = sqlx::query_file_scalar!(
            "src/store/persistence/fleet/lifecycle_sql/record_open_run_surface.sql",
            self.run_id,
            refs.workspace_ref,
            refs.tab_ref,
        )
        .fetch_optional(&mut *self.transaction)
        .await?;
        updated.map(|_| ()).ok_or_else(|| {
            PersistError::DataInvariant(format!(
                "run {:?} is missing or closed; launch opening was not committed",
                self.run_id
            ))
        })
    }

    pub async fn record_run(mut self, run: &NewRun) -> Result<(), PersistError> {
        if run.run_id != self.run_id {
            return Err(PersistError::DataInvariant(format!(
                "run {:?} does not match admitted run {:?}; metadata was not recorded",
                run.run_id, self.run_id
            )));
        }
        let recorded = sqlx::query_file_scalar!(
            "src/store/persistence/fleet/lifecycle_sql/update_open_run.sql",
            run.run_id,
            run.project_path,
            run.backend,
            run.workspace_ref,
            run.tab_ref,
        )
        .fetch_optional(&mut *self.transaction)
        .await?;
        if recorded.is_none() {
            return Err(PersistError::DataInvariant(format!(
                "run {:?} is missing or already closed; metadata was not recorded",
                run.run_id
            )));
        }
        self.transaction.commit().await?;
        Ok(())
    }
}

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn begin_fleet_launch(
        self,
        run_id: &str,
    ) -> Result<Option<FleetLaunchAdmission>, PersistError> {
        crate::metrics::time_db("launch_begin_fleet_launch", async move {
            let mut transaction = self.pool.begin().await?;
            let rows = sqlx::query_file_as!(
                FleetRunAdmission,
                "src/store/persistence/fleet/lifecycle_sql/lock_run_admission.sql",
                run_id,
            )
            .fetch_all(&mut *transaction)
            .await?;
            if rows.is_empty() {
                transaction.rollback().await?;
                return Ok(None);
            }
            if rows.len() != 1 || !rows.iter().all(FleetRunAdmission::admits_work) {
                return Err(PersistError::DataInvariant(format!(
                    "run {run_id:?} has no unambiguous starting/running fleet; launch admission refused"
                )));
            }
            Ok(Some(FleetLaunchAdmission {
                transaction,
                run_id: run_id.to_owned(),
            }))
        })
        .await
    }
}
