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
    pub label: String,
    pub name: String,
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
                    label: row.label.unwrap_or_default(),
                    name: row.name.unwrap_or_default(),
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
        let kinds = sqlx::query_scalar!(
            "SELECT k.kind FROM memory.knowledge_kinds k WHERE k.enabled ORDER BY k.kind"
        )
        .fetch_all(self.pool())
        .await?;
        let mut changed = 0;
        for kind in &kinds {
            changed += sqlx::query_scalar!(
                r#"SELECT memory.reconcile_knowledge_items($1) AS "changed!""#,
                kind
            )
            .fetch_one(self.pool())
            .await?;
        }
        if changed > 0 {
            self.forget_knowledge_coverage();
        }
        Ok(changed)
    }

    pub async fn keep_unchanged_knowledge_embedding(
        self,
        source: &KnowledgeEmbeddingSource,
        model: &str,
        hash: &str,
    ) -> Result<bool, PersistError> {
        let mut tx = self.pool().begin().await?;
        let kept = sqlx::query_scalar!(
            r#"UPDATE memory.knowledge_embedding_state s
                  SET reference = $4, project_id = $5
                WHERE s.item_id = $1 AND s.model_key = $2 AND s.source_hash = $3
                  AND s.completed_at IS NOT NULL
                  AND EXISTS (SELECT 1 FROM memory.knowledge_embedding_chunks c WHERE c.state_id = s.id)
               RETURNING s.id"#,
            source.item_id,
            model,
            hash,
            &source.reference,
            source.project_id
        )
        .fetch_optional(&mut *tx)
        .await?
        .is_some();
        if kept {
            mark_item_embedded(&mut tx, source, model).await?;
        }
        tx.commit().await?;
        Ok(kept)
    }

    pub async fn save_knowledge_embeddings(
        self,
        source: &KnowledgeEmbeddingSource,
        model: &str,
        hash: &str,
        chunks: Value,
    ) -> Result<(), PersistError> {
        let mut tx = self.pool().begin().await?;
        let (state_id, hash_changed) = upsert_embedding_state(&mut tx, source, model, hash).await?;
        replace_embedding_chunks(&mut tx, state_id, chunks).await?;
        if hash_changed {
            queue_item_for_consolidation(&mut tx, source.item_id).await?;
        }
        mark_item_embedded(&mut tx, source, model).await?;
        tx.commit().await?;
        self.forget_knowledge_coverage();
        Ok(())
    }

    pub async fn share_twin_knowledge_embeddings(
        self,
        item_ids: &[i64],
        model: &str,
    ) -> Result<Vec<i64>, PersistError> {
        if item_ids.is_empty() {
            return Ok(Vec::new());
        }
        let served = sqlx::query_scalar!(
            r#"SELECT memory.share_twin_embeddings($1::bigint[], $2::text) AS "served!""#,
            item_ids,
            model
        )
        .fetch_one(self.pool())
        .await?;
        if !served.is_empty() {
            self.forget_knowledge_coverage();
        }
        Ok(served)
    }

    pub async fn fail_knowledge_embedding(
        self,
        source: &KnowledgeEmbeddingSource,
        model: &str,
        error: &str,
    ) -> Result<(), PersistError> {
        sqlx::query!(
            "UPDATE memory.knowledge_items
                SET attempts = attempts + 1, last_error = $3, last_error_at = now(), project_id = $4, model_key = $5,
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

type Tx<'a> = sqlx::Transaction<'a, sqlx::Postgres>;

async fn upsert_embedding_state(
    tx: &mut Tx<'_>,
    source: &KnowledgeEmbeddingSource,
    model: &str,
    hash: &str,
) -> Result<(i64, bool), PersistError> {
    let row = sqlx::query!(
        "INSERT INTO memory.knowledge_embedding_state
             (item_id, source_kind, source_id, reference, project_id, model_key, completed_at,
              name, label, search_tsv, source_hash)
         SELECT $1::bigint, k.kind, $3::bigint, $4::text, $5::bigint, $6::text, now(),
                $7::text, $8::text,
                CASE WHEN k.built_in THEN NULL ELSE to_tsvector('english', $9::text || ' ' || $10::text) END,
                $11::text
           FROM memory.knowledge_kinds k
          WHERE k.kind = $2::text
         ON CONFLICT (item_id) DO UPDATE SET
             reference = EXCLUDED.reference, project_id = EXCLUDED.project_id,
             model_key = EXCLUDED.model_key, completed_at = now(), attempts = 0, last_error = NULL,
             name = EXCLUDED.name, label = EXCLUDED.label, search_tsv = EXCLUDED.search_tsv,
             source_hash = EXCLUDED.source_hash
         RETURNING id, old.source_hash IS DISTINCT FROM new.source_hash AS \"hash_changed!\"",
        source.item_id,
        &source.source_kind,
        source.source_id,
        &source.reference,
        source.project_id,
        model,
        &source.name,
        &source.label,
        &source.header,
        &source.body,
        hash
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok((row.id, row.hash_changed))
}

async fn queue_item_for_consolidation(tx: &mut Tx<'_>, item_id: i64) -> Result<(), PersistError> {
    sqlx::query!(
        "SELECT memory.queue_knowledge_consolidation(ARRAY[$1::bigint])",
        item_id
    )
    .fetch_one(&mut **tx)
    .await?;
    Ok(())
}

async fn replace_embedding_chunks(
    tx: &mut Tx<'_>,
    state_id: i64,
    chunks: Value,
) -> Result<(), PersistError> {
    sqlx::query!(
        "DELETE FROM memory.knowledge_embedding_chunks WHERE state_id = $1",
        state_id
    )
    .execute(&mut **tx)
    .await?;
    sqlx::query!(
        "INSERT INTO memory.knowledge_embedding_chunks(state_id, ordinal, body, embedding)
         SELECT $1, x.ordinal, x.body, x.embedding::public.vector
           FROM jsonb_to_recordset($2) x(ordinal integer, body text, embedding text)",
        state_id,
        chunks
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn mark_item_embedded(
    tx: &mut Tx<'_>,
    source: &KnowledgeEmbeddingSource,
    model: &str,
) -> Result<(), PersistError> {
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
    .execute(&mut **tx)
    .await?;
    Ok(())
}
