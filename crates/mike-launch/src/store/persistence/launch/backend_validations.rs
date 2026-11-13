use super::{BackendValidationRow, NewBackendValidation};
use crate::store::persistence::PersistError;

impl crate::store::persistence::launch_repo::LaunchDb<'_> {
    pub async fn insert_backend_validation(
        self,
        v: &NewBackendValidation,
    ) -> Result<i64, PersistError> {
        crate::metrics::time_db("launch_insert_backend_validation", async move {
            let id = sqlx::query_file_scalar!(
                "src/store/persistence/launch/insert_backend_validation.sql",
                v.backend,
                v.passed,
                v.version,
                v.protocol,
                v.checks,
                v.started_at,
                v.finished_at,
                v.source,
            )
            .fetch_one(&self.pool)
            .await?;
            Ok(id)
        })
        .await
    }

    pub async fn latest_backend_validation(
        self,
        backend: &str,
    ) -> Result<Option<BackendValidationRow>, PersistError> {
        crate::metrics::time_db("launch_latest_backend_validation", async move {
            let row = sqlx::query_file_as!(
                BackendValidationRow,
                "src/store/persistence/launch/latest_backend_validation.sql",
                backend,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }
}
