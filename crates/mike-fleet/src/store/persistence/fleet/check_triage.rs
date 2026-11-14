use crate::store::persistence::PersistError;

impl crate::store::persistence::fleet_repo::FleetDb<'_> {
    pub async fn set_tree_check_triage(self, id: i64, triage: &str) -> Result<bool, PersistError> {
        crate::metrics::time_db("launch_set_tree_check_triage", async move {
            let done = sqlx::query!(
                "UPDATE launch.tree_checks SET triage = $2 WHERE id = $1 AND triage IS NULL",
                id,
                triage,
            )
            .execute(&self.pool)
            .await?;
            Ok(done.rows_affected() > 0)
        })
        .await
    }

    pub async fn tree_check_triage(self, id: i64) -> Result<Option<String>, PersistError> {
        crate::metrics::time_db("launch_tree_check_triage", async move {
            let found =
                sqlx::query_scalar!("SELECT triage FROM launch.tree_checks WHERE id = $1", id)
                    .fetch_optional(&self.pool)
                    .await?;
            Ok(found.flatten())
        })
        .await
    }
}
