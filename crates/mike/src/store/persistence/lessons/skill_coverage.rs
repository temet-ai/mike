use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub async fn list_topic_lessons_withheld_from_content(&self) -> Result<Vec<i64>, PersistError> {
        crate::metrics::time_db("list_topic_lessons_withheld_from_content", async move {
            let ids = sqlx::query_file_scalar!("src/store/persistence/lessons/skill_coverage.sql")
                .fetch_all(&self.pool)
                .await?;
            Ok(ids)
        })
        .await
    }
}
