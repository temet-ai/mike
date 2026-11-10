use crate::store::persistence::{PersistError, PersistHandle};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};

pub(crate) const PENDING_BATCH: i64 = 16;
pub(crate) const KNOWLEDGE_COUNT_CAP: i64 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KnowledgeStateCounts {
    pub ready: i64,
    pub failed: i64,
}

pub(crate) struct KnowledgeEmbeddingSource {
    pub reference: String,
    pub source_kind: String,
    pub source_id: i64,
    pub source_updated_at: Option<DateTime<Utc>>,
    pub source_seq: Option<i64>,
    pub source_hash: Option<String>,
    pub body: String,
}

impl PersistHandle {
    pub(crate) async fn pending_knowledge_embeddings(
        &self,
        model: &str,
    ) -> Result<Vec<KnowledgeEmbeddingSource>, PersistError> {
        Ok(sqlx::query_file_as!(
            KnowledgeEmbeddingSource,
            "src/store/persistence/project_knowledge/pending_embeddings.sql",
            model,
            PENDING_BATCH
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub(crate) async fn save_knowledge_embeddings(
        &self,
        source: &KnowledgeEmbeddingSource,
        model: &str,
        chunks: Value,
    ) -> Result<(), PersistError> {
        let mut tx = self.pool().begin().await?;
        let state_id = sqlx::query_scalar!(
            "INSERT INTO memory.knowledge_embedding_state
                 (source_kind, source_id, source_updated_at, source_seq, source_hash, model_key, completed_at)
             VALUES ($1, $2, $3, $4, $5, $6, now())
             ON CONFLICT (source_kind, source_id) DO UPDATE SET
                 source_updated_at = EXCLUDED.source_updated_at, source_seq = EXCLUDED.source_seq,
                 source_hash = EXCLUDED.source_hash, model_key = EXCLUDED.model_key,
                 completed_at = now(), attempts = 0, last_error = NULL
             RETURNING id",
            &source.source_kind,
            source.source_id,
            source.source_updated_at,
            source.source_seq,
            source.source_hash.as_deref(),
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
        tx.commit().await?;
        self.forget_knowledge_coverage();
        Ok(())
    }

    pub(crate) async fn fail_knowledge_embedding(
        &self,
        source: &KnowledgeEmbeddingSource,
        model: &str,
        error: &str,
    ) -> Result<(), PersistError> {
        sqlx::query!(
            "INSERT INTO memory.knowledge_embedding_state AS e
                 (source_kind, source_id, source_updated_at, source_seq, source_hash, model_key,
                  attempts, next_attempt_at, last_error)
             VALUES ($1, $2, $3, $4, $5, $6, 1, now() + interval '30 seconds', $7)
             ON CONFLICT (source_kind, source_id) DO UPDATE SET
                 source_updated_at = EXCLUDED.source_updated_at, source_seq = EXCLUDED.source_seq,
                 source_hash = EXCLUDED.source_hash, model_key = EXCLUDED.model_key,
                 completed_at = NULL, last_error = EXCLUDED.last_error,
                 attempts = CASE
                     WHEN e.model_key = EXCLUDED.model_key
                      AND e.source_updated_at IS NOT DISTINCT FROM EXCLUDED.source_updated_at
                      AND e.source_seq IS NOT DISTINCT FROM EXCLUDED.source_seq
                      AND e.source_hash IS NOT DISTINCT FROM EXCLUDED.source_hash
                     THEN e.attempts + 1 ELSE 1 END,
                 next_attempt_at = now() + LEAST(3600, 30 * power(2, LEAST(e.attempts, 7))) * interval '1 second'",
            &source.source_kind,
            source.source_id,
            source.source_updated_at,
            source.source_seq,
            source.source_hash.as_deref(),
            model,
            error
        )
        .execute(self.pool())
        .await?;
        self.forget_knowledge_coverage();
        Ok(())
    }

    pub(crate) async fn knowledge_embedding_url_candidates(
        &self,
    ) -> Result<Vec<String>, PersistError> {
        Ok(sqlx::query_scalar!(
            r#"SELECT url AS "url!" FROM (
               SELECT unnest(urls_seen) AS url FROM memory.semantic_embedders
               UNION
               SELECT value #>> '{}' FROM config.settings WHERE key = 'embedding.url'
             ) candidates WHERE url IS NOT NULL"#
        )
        .fetch_all(self.pool())
        .await?)
    }

    pub(crate) async fn rekey_knowledge_embeddings(
        &self,
        legacy_keys: &[String],
        key: &str,
    ) -> Result<u64, PersistError> {
        let rekeyed = sqlx::query!(
            "UPDATE memory.knowledge_embedding_state SET model_key = $2
             WHERE model_key = ANY($1) AND model_key <> $2",
            legacy_keys,
            key
        )
        .execute(self.pool())
        .await?
        .rows_affected();
        self.forget_knowledge_coverage();
        Ok(rekeyed)
    }

    pub(crate) async fn knowledge_embedding_state_counts(
        &self,
        model: &str,
    ) -> Result<KnowledgeStateCounts, PersistError> {
        let counts = sqlx::query!(
            r#"SELECT
                 (SELECT count(*) FROM (
                     SELECT 1 FROM memory.knowledge_embedding_state e
                      WHERE e.model_key = $1 AND e.completed_at IS NOT NULL
                      LIMIT $2) ready) AS "ready!",
                 (SELECT count(*) FROM (
                     SELECT 1 FROM memory.knowledge_embedding_state e
                     CROSS JOIN LATERAL (
                         SELECT 1 FROM memory.knowledge_sources_with_sessions_v s
                          WHERE s.source_kind = e.source_kind AND s.source_id = e.source_id
                            AND s.source_updated_at IS NOT DISTINCT FROM e.source_updated_at
                            AND s.source_seq IS NOT DISTINCT FROM e.source_seq
                            AND s.source_hash IS NOT DISTINCT FROM e.source_hash
                          LIMIT 1) current_source
                      WHERE e.model_key = $1 AND e.last_error IS NOT NULL
                      LIMIT $2) failed) AS "failed!""#,
            model,
            KNOWLEDGE_COUNT_CAP + 1
        )
        .fetch_one(self.pool())
        .await?;
        Ok(KnowledgeStateCounts {
            ready: counts.ready,
            failed: counts.failed,
        })
    }

    pub(crate) async fn knowledge_embedding_health(
        &self,
        project: Option<i64>,
        model: &str,
    ) -> Result<Value, PersistError> {
        crate::metrics::time_db(
            "knowledge_embedding_health",
            self.count_embedding_health(project, model),
        )
        .await
    }

    async fn count_embedding_health(
        &self,
        project: Option<i64>,
        model: &str,
    ) -> Result<Value, PersistError> {
        let (total, ready, failed) = match project {
            Some(project) => {
                let counts = sqlx::query_file!(
                    "src/store/persistence/project_knowledge/embedding_health_project.sql",
                    project,
                    model
                )
                .fetch_one(self.pool())
                .await?;
                (counts.total, counts.ready, counts.failed)
            }
            None => {
                let counts = sqlx::query_file!(
                    "src/store/persistence/project_knowledge/embedding_health_all.sql",
                    model
                )
                .fetch_one(self.pool())
                .await?;
                (counts.total, counts.ready, counts.failed)
            }
        };
        Ok(json!({"total":total,"ready":ready,"pending":total-ready,"failed":failed}))
    }
}
