use std::collections::HashMap;

use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub(crate) async fn knowledge_rerank_passages(
        &self,
        references: &[String],
        vector: Option<&(String, String, String)>,
    ) -> Result<HashMap<String, String>, PersistError> {
        let rows = sqlx::query_file!(
            "src/store/persistence/project_knowledge/rerank_passages.sql",
            references,
            vector.map(|v| v.0.as_str()),
            vector.map(|v| v.1.as_str()),
            vector.map(|v| v.2.as_str()),
        )
        .fetch_all(self.pool())
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.reference, row.body))
            .collect())
    }
}
