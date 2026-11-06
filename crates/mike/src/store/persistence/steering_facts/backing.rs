use super::super::{PersistError, PersistHandle};
use super::{BackingRow, FactKey};

impl PersistHandle {
    pub async fn steering_backing_rows(
        &self,
        keys: &[FactKey],
        count_unregistered: bool,
    ) -> Result<Vec<BackingRow>, PersistError> {
        if keys.is_empty() {
            return Ok(Vec::new());
        }
        let turns: Vec<i64> = keys.iter().map(|k| k.turn_id).collect();
        let sessions: Vec<i64> = keys.iter().map(|k| k.session_id).collect();
        let at: Vec<chrono::DateTime<chrono::Utc>> = keys.iter().map(|k| k.as_of).collect();
        crate::metrics::time_db("steering_backing_rows", async move {
            Ok(sqlx::query_file_as!(
                BackingRow,
                "src/store/persistence/steering_facts/backing.sql",
                &turns,
                &sessions,
                &at,
                count_unregistered,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}
