use crate::store::persistence::PersistError;

#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CheckTriage {
    pub id: i64,
    pub finished_at: Option<chrono::DateTime<chrono::Utc>>,
    pub exit_code: Option<i32>,
    pub askers: Vec<String>,
    pub triage: String,
}

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

    pub async fn run_check_triage(
        self,
        run_id: &str,
        limit: i64,
    ) -> Result<Vec<CheckTriage>, PersistError> {
        crate::metrics::time_db("launch_run_check_triage", async move {
            Ok(sqlx::query_as!(
                CheckTriage,
                r#"SELECT id AS "id!", finished_at, exit_code, askers AS "askers!",
                          triage AS "triage!"
                     FROM launch.tree_checks
                    WHERE run_id = $1 AND triage IS NOT NULL
                    ORDER BY id DESC
                    LIMIT $2"#,
                run_id,
                limit,
            )
            .fetch_all(&self.pool)
            .await?)
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
