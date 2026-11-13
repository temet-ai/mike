use super::{NewRun, RunRow};
use crate::store::persistence::PersistError;

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn upsert_run(self, run: &NewRun) -> Result<(), PersistError> {
        crate::metrics::time_db("launch_upsert_run", async move {
            sqlx::query_file!(
                "src/store/persistence/launch/upsert_run.sql",
                run.run_id,
                run.project_path,
                run.backend,
                run.workspace_ref,
                run.tab_ref,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn find_run(self, run_id: &str) -> Result<Option<RunRow>, PersistError> {
        crate::metrics::time_db("launch_find_run", async move {
            let row =
                sqlx::query_file_as!(RunRow, "src/store/persistence/launch/find_run.sql", run_id,)
                    .fetch_optional(&self.pool)
                    .await?;
            Ok(row)
        })
        .await
    }

    pub async fn close_run(self, run_id: &str) -> Result<(), PersistError> {
        crate::metrics::time_db("launch_close_run", async move {
            let hit =
                sqlx::query_file_scalar!("src/store/persistence/launch/close_run.sql", run_id)
                    .fetch_optional(&self.pool)
                    .await?;
            hit.map(|_| ()).ok_or_else(|| {
                PersistError::DataInvariant(format!(
                    "launch.runs run_id {run_id:?} does not exist, so it was not closed"
                ))
            })
        })
        .await
    }
}
