use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub(crate) async fn knowledge_nearest_first(
        &self,
        project_id: i64,
        model_key: &str,
    ) -> Result<bool, PersistError> {
        self.knowledge_memos
            .nearest_first
            .get_or_fetch((project_id, model_key.to_owned()), || {
                self.count_nearest_first(project_id, model_key)
            })
            .await
    }

    async fn count_nearest_first(
        &self,
        project_id: i64,
        model_key: &str,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("knowledge_nearest_first", async {
            let enabled = sqlx::query_file_scalar!(
                "src/store/persistence/project_knowledge/nearest_first.sql",
                project_id,
                model_key,
            )
            .fetch_one(self.pool())
            .await?;
            Ok(enabled)
        })
        .await
    }
}
