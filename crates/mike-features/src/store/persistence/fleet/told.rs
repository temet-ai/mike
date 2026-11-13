use crate::store::persistence::PersistError;

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn told_keys(self, run_id: &str) -> Result<Vec<String>, PersistError> {
        crate::metrics::time_db("launch_told_keys", async move {
            let rows =
                sqlx::query_file!("src/store/persistence/fleet/told_sql/told_keys.sql", run_id)
                    .fetch_all(&self.pool)
                    .await?;
            Ok(rows.into_iter().map(|r| r.what).collect())
        })
        .await
    }

    pub async fn recorded_introductions(self, again: &[i64]) -> Result<Vec<i64>, PersistError> {
        if again.is_empty() {
            return Ok(Vec::new());
        }
        crate::metrics::time_db("launch_recorded_introductions", async move {
            let rows = sqlx::query_file!(
                "src/store/persistence/fleet/told_sql/recorded_introductions.sql",
                again
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows.into_iter().map(|r| r.again_use_id).collect())
        })
        .await
    }

    pub async fn release_introduction(self, again_use_id: i64) -> Result<(), PersistError> {
        crate::metrics::time_db("launch_release_introduction", async move {
            sqlx::query_file!(
                "src/store/persistence/fleet/told_sql/release_introduction.sql",
                again_use_id
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn mark_told(self, run_id: &str, what: &[String]) -> Result<(), PersistError> {
        if what.is_empty() {
            return Ok(());
        }
        crate::metrics::time_db("launch_mark_told", async move {
            sqlx::query_file!(
                "src/store/persistence/fleet/told_sql/mark_told.sql",
                run_id,
                what
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }
}
