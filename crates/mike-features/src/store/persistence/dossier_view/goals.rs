use super::super::PersistError;
use super::GoalsRow;

impl crate::store::persistence::dossier_repo::DossierDb<'_> {
    pub async fn fetch_session_goals_row(
        self,
        session_db_id: i64,
    ) -> Result<Option<GoalsRow>, PersistError> {
        crate::metrics::time_db("fetch_session_goals_row", async move {
            let row = sqlx::query_as!(
                GoalsRow,
                "SELECT goal, instructions, updated_at
                   FROM sessions.session_goals
                  WHERE session_id = $1",
                session_db_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }
}
