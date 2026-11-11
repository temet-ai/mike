use std::collections::HashMap;

use crate::store::persistence::{PersistError, PersistHandle};

impl PersistHandle {
    pub(crate) async fn knowledge_rerank_passages(
        &self,
        references: &[String],
        vector: Option<&super::QueryEmbedding>,
    ) -> Result<HashMap<String, String>, PersistError> {
        let accepted = vector.map(|v| v.1.accepted());
        let rows = crate::metrics::time_db("knowledge_rerank_passages", async {
            let rows = sqlx::query_file!(
                "src/store/persistence/project_knowledge/rerank_passages.sql",
                references,
                vector.map(|v| v.0.as_str()),
                accepted.as_deref(),
                vector.map(|v| v.2.as_str()),
            )
            .fetch_all(self.pool())
            .await?;
            Ok::<_, PersistError>(rows)
        })
        .await?;
        Ok(rows
            .into_iter()
            .map(|row| (row.reference, row.body))
            .collect())
    }
}
