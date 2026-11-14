use crate::store::persistence::PersistError;

const CLOSE_UNOPENED_LAUNCH: &str = "
UPDATE launch.launches
   SET closed_at = COALESCE(closed_at, now()),
       close_reason = COALESCE(close_reason, $3),
       close_process_exited = CASE
           WHEN close_process_exited IS TRUE OR $4::boolean IS TRUE THEN TRUE
           WHEN close_process_exited IS FALSE OR $4::boolean IS FALSE THEN FALSE
           ELSE NULL
       END,
       result = CASE WHEN close_process_exited IS TRUE OR $4::boolean IS TRUE
                     THEN 'failed' ELSE result END,
       reason = CASE WHEN close_process_exited IS TRUE OR $4::boolean IS TRUE
                     THEN COALESCE(reason, 'closed_before_open_recorded') ELSE reason END
 WHERE id = $1
   AND run_id = $2
   AND NOT opened
RETURNING id
";

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
            if hit.is_some() {
                return Ok(());
            }
            let unopened = sqlx::query_scalar::<_, i64>(CLOSE_UNOPENED_LAUNCH)
                .bind(launch_id)
                .bind(run_id)
                .bind(reason)
                .bind(process_exited)
                .fetch_optional(&self.pool)
                .await?;
            unopened.map(|_| ()).ok_or_else(|| {
                PersistError::DataInvariant(format!(
                    "launch {launch_id} does not belong to run {run_id:?}; closure was not recorded"
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
