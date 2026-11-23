use crate::store::persistence::PersistError;
use crate::store::persistence::recall_embedding::set_vector_scan_limits;
use chrono::{DateTime, Utc};

#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceChunk {
    pub reference: String,
    pub kind: String,
    pub body: String,
    pub similarity: f64,
    pub dated_at: DateTime<Utc>,
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn nearest_evidence_chunks(
        self,
        project_id: i64,
        embedding: &str,
        k: i64,
    ) -> Result<Vec<EvidenceChunk>, PersistError> {
        let mut tx = self.pool().begin().await?;
        set_vector_scan_limits(&mut tx).await?;
        let rows = sqlx::query_file!(
            "src/store/persistence/consolidation/nearest_evidence.sql",
            project_id,
            embedding,
            k
        )
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(rows
            .into_iter()
            .map(|row| EvidenceChunk {
                reference: row.reference,
                kind: row.kind,
                body: row.body,
                similarity: row.similarity,
                dated_at: row.dated_at,
            })
            .collect())
    }
}
