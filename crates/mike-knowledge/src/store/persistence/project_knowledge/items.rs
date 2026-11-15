#[cfg(test)]
use super::PENDING_BATCH;
use crate::store::persistence::PersistError;
use chrono::{DateTime, Utc};
use serde_json::Value;

#[derive(Debug, Clone)]
pub struct KnowledgeEmbeddingSource {
    pub item_id: i64,
    pub changed_at: DateTime<Utc>,
    pub reference: String,
    pub source_kind: String,
    pub source_id: i64,
    pub project_id: Option<i64>,
    pub header: String,
    pub body: String,
}

#[derive(Debug, Default)]
pub struct PendingKnowledge {
    pub picked: usize,
    pub sources: Vec<KnowledgeEmbeddingSource>,
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    #[cfg(test)]
    pub async fn pending_knowledge_embeddings(
        self,
        model: &str,
    ) -> Result<Vec<KnowledgeEmbeddingSource>, PersistError> {
        self.mark_knowledge_items_for_model(model).await?;
        Ok(self
            .pending_knowledge_embeddings_limited(PENDING_BATCH)
            .await?
            .sources)
    }

    pub async fn pending_knowledge_embeddings_limited(
        self,
        limit: i64,
    ) -> Result<PendingKnowledge, PersistError> {
        let rows = sqlx::query_file!(
            "src/store/persistence/project_knowledge/pending_embeddings.sql",
            limit
        )
        .fetch_all(self.pool())
        .await?;
        let mut pending = PendingKnowledge {
            picked: rows.len(),
            sources: Vec::with_capacity(rows.len()),
        };
        let mut gone = (Vec::new(), Vec::new());
        for row in rows {
            match (row.reference, row.body) {
                (Some(reference), Some(body)) => pending.sources.push(KnowledgeEmbeddingSource {
                    item_id: row.item_id,
                    changed_at: row.changed_at,
                    reference,
                    source_kind: row.source_kind,
                    source_id: row.source_id,
                    project_id: row.project_id,
                    header: row.header.unwrap_or_default(),
                    body,
                }),
                _ => {
                    gone.0.push(row.item_id);
                    gone.1.push(row.changed_at);
                }
            }
        }
        self.forget_gone_knowledge_items(&gone.0, &gone.1).await?;
        Ok(pending)
    }

    async fn forget_gone_knowledge_items(
        self,
        ids: &[i64],
        changed_at: &[DateTime<Utc>],
    ) -> Result<(), PersistError> {
        if ids.is_empty() {
            return Ok(());
        }
        sqlx::query!(
            "DELETE FROM memory.knowledge_items i
              USING unnest($1::bigint[], $2::timestamptz[]) AS g(id, changed_at)
              WHERE i.id = g.id AND i.changed_at <= g.changed_at",
            ids,
            changed_at
        )
        .execute(self.pool())
        .await?;
        self.forget_knowledge_coverage();
        Ok(())
    }

    pub async fn mark_knowledge_items_for_model(self, model: &str) -> Result<u64, PersistError> {
        let marked = sqlx::query!(
            "UPDATE memory.knowledge_items
                SET needs_embedding = true, ready_at = now(), attempts = 0, last_error = NULL,
                    changed_at = clock_timestamp()
              WHERE model_key IS DISTINCT FROM $1 AND model_key IS NOT NULL",
            model
        )
        .execute(self.pool())
        .await?
        .rows_affected();
        if marked > 0 {
            self.forget_knowledge_coverage();
        }
        Ok(marked)
    }

    pub async fn reconcile_knowledge_items(self) -> Result<i64, PersistError> {
        let changed = sqlx::query_scalar!(
            r#"SELECT COALESCE(sum(memory.reconcile_knowledge_items(k.kind)), 0)::bigint AS "changed!"
                 FROM memory.knowledge_kinds k WHERE k.enabled"#
        )
        .fetch_one(self.pool())
        .await?;
        if changed > 0 {
            self.forget_knowledge_coverage();
        }
        Ok(changed)
    }

    pub async fn save_knowledge_embeddings(
        self,
        source: &KnowledgeEmbeddingSource,
        model: &str,
        chunks: Value,
    ) -> Result<(), PersistError> {
        let mut tx = self.pool().begin().await?;
        let state_id = sqlx::query_scalar!(
            "INSERT INTO memory.knowledge_embedding_state
                 (item_id, source_kind, source_id, reference, project_id, model_key, completed_at)
             VALUES ($1, $2, $3, $4, $5, $6, now())
             ON CONFLICT (item_id) DO UPDATE SET
                 reference = EXCLUDED.reference, project_id = EXCLUDED.project_id,
                 model_key = EXCLUDED.model_key, completed_at = now(), attempts = 0, last_error = NULL
             RETURNING id",
            source.item_id,
            &source.source_kind,
            source.source_id,
            &source.reference,
            source.project_id,
            model
        )
        .fetch_one(&mut *tx)
        .await?;
        sqlx::query!(
            "DELETE FROM memory.knowledge_embedding_chunks WHERE state_id = $1",
            state_id
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(
            "INSERT INTO memory.knowledge_embedding_chunks(state_id, ordinal, body, embedding)
             SELECT $1, x.ordinal, x.body, x.embedding::public.vector
               FROM jsonb_to_recordset($2) x(ordinal integer, body text, embedding text)",
            state_id,
            chunks
        )
        .execute(&mut *tx)
        .await?;
        sqlx::query!(
            "UPDATE memory.knowledge_items
                SET needs_embedding = changed_at > $2, project_id = $3, attempts = 0, last_error = NULL,
                    model_key = $4
              WHERE id = $1",
            source.item_id,
            source.changed_at,
            source.project_id,
            model
        )
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        self.forget_knowledge_coverage();
        Ok(())
    }

    pub async fn fail_knowledge_embedding(
        self,
        source: &KnowledgeEmbeddingSource,
        model: &str,
        error: &str,
    ) -> Result<(), PersistError> {
        sqlx::query!(
            "UPDATE memory.knowledge_items
                SET attempts = attempts + 1, last_error = $3, project_id = $4, model_key = $5,
                    ready_at = now() + LEAST(3600, 30 * power(2, LEAST(attempts, 7))) * interval '1 second',
                    needs_embedding = attempts + 1 < 10 OR $3 LIKE '[transient]%'
              WHERE id = $1 AND changed_at <= $2",
            source.item_id,
            source.changed_at,
            error,
            source.project_id,
            model
        )
        .execute(self.pool())
        .await?;
        self.forget_knowledge_coverage();
        Ok(())
    }
}
