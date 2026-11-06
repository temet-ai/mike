use super::super::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn fetch_coordinator_for_seats(
        &self,
        session_ids: &[i64],
    ) -> Result<Vec<(i64, Option<uuid::Uuid>)>, PersistError> {
        crate::metrics::time_db("fetch_coordinator_for_seats", async move {
            let rows = sqlx::query_file!(
                "src/store/persistence/steering_facts/coordinator.sql",
                session_ids,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| (r.session_id, r.coordinator))
                .collect())
        })
        .await
    }
}
