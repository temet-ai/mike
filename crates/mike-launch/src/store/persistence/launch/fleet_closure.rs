use crate::store::persistence::PersistError;

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn record_fleet_launch_closure(
        self,
        launch_id: i64,
        run_id: &str,
        reason: &str,
        process_exited: Option<bool>,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("launch_record_fleet_launch_closure", async move {
            let hit = sqlx::query_file_scalar!(
                "src/store/persistence/launch/record_fleet_launch_closure.sql",
                launch_id,
                run_id,
                reason,
                process_exited,
            )
            .fetch_optional(&self.pool)
            .await?;
            hit.map(|_| ()).ok_or_else(|| {
                PersistError::DataInvariant(format!(
                    "opened launch {launch_id} does not belong to run {run_id:?}; closure was not recorded"
                ))
            })
        })
        .await
    }

    pub async fn close_never_started_fleet_launch(
        self,
        launch_id: i64,
        run_id: &str,
        reason: &str,
        older_than_seconds: f64,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("launch_close_never_started_fleet_launch", async move {
            let hit = sqlx::query_file_scalar!(
                "src/store/persistence/launch/close_never_started_fleet_launch.sql",
                launch_id,
                run_id,
                reason,
                older_than_seconds,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(hit.is_some())
        })
        .await
    }
}
