use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::PersistError;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct EmbedderRow {
    pub id: i64,
    pub embedder_key: String,
    pub model: String,
    pub dim: i32,
    pub chunk_max_chars: i32,
    pub prefix_version: String,
    pub urls_seen: Vec<String>,
    pub canary_version: String,
    pub canary_vectors: serde_json::Value,
    pub status: String,
    pub first_seen_at: DateTime<Utc>,
    pub last_checked_at: DateTime<Utc>,
    pub copy_valid_from: Option<DateTime<Utc>>,
    pub copy_verify: Option<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct NewEmbedder<'a> {
    pub embedder_key: &'a str,
    pub model: &'a str,
    pub dim: i32,
    pub chunk_max_chars: i32,
    pub prefix_version: &'a str,
    pub url: &'a str,
    pub canary_version: &'a str,
    pub canary_vectors: serde_json::Value,
    pub first_seen_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct VerifySample {
    pub id: i64,
    pub chunk_text: String,
    pub embedding: String,
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn current_semantic_embedder(self) -> Result<Option<EmbedderRow>, PersistError> {
        crate::metrics::time_db("current_semantic_embedder", async move {
            Ok(sqlx::query_as!(
                EmbedderRow,
                "SELECT id, embedder_key, model, dim, chunk_max_chars, prefix_version, urls_seen,
                        canary_version, canary_vectors, status, first_seen_at, last_checked_at,
                        copy_valid_from, copy_verify
                   FROM memory.semantic_embedders WHERE status = 'current'"
            )
            .fetch_optional(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn list_semantic_embedders(self) -> Result<Vec<EmbedderRow>, PersistError> {
        crate::metrics::time_db("list_semantic_embedders", async move {
            Ok(sqlx::query_as!(
                EmbedderRow,
                "SELECT id, embedder_key, model, dim, chunk_max_chars, prefix_version, urls_seen,
                        canary_version, canary_vectors, status, first_seen_at, last_checked_at,
                        copy_valid_from, copy_verify
                   FROM memory.semantic_embedders ORDER BY id DESC"
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn insert_semantic_embedder(self, e: &NewEmbedder<'_>) -> Result<i64, PersistError> {
        crate::metrics::time_db("insert_semantic_embedder", async move {
            let mut tx = self.pool.begin().await?;
            sqlx::query!(
                "UPDATE memory.semantic_embedders SET status = 'retired' WHERE status = 'current'"
            )
            .execute(&mut *tx)
            .await?;
            let id = sqlx::query_scalar!(
                "INSERT INTO memory.semantic_embedders
                    (embedder_key, model, dim, chunk_max_chars, prefix_version, urls_seen,
                     canary_version, canary_vectors, first_seen_at)
                 VALUES ($1, $2, $3, $4, $5, ARRAY[$6::text], $7, $8, $9)
                 RETURNING id",
                e.embedder_key,
                e.model,
                e.dim,
                e.chunk_max_chars,
                e.prefix_version,
                e.url,
                e.canary_version,
                &e.canary_vectors,
                e.first_seen_at,
            )
            .fetch_one(&mut *tx)
            .await?;
            tx.commit().await?;
            Ok(id)
        })
        .await
    }

    pub async fn touch_semantic_embedder(self, id: i64, url: &str) -> Result<(), PersistError> {
        crate::metrics::time_db("touch_semantic_embedder", async move {
            sqlx::query!(
                "UPDATE memory.semantic_embedders
                    SET last_checked_at = now(),
                        urls_seen = CASE WHEN $2 = ANY(urls_seen) THEN urls_seen
                                         ELSE array_append(urls_seen, $2) END
                  WHERE id = $1",
                id,
                url,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn set_semantic_copy_window(
        self,
        id: i64,
        from: DateTime<Utc>,
        verify: &serde_json::Value,
    ) -> Result<bool, PersistError> {
        crate::metrics::time_db("set_semantic_copy_window", async move {
            let r = sqlx::query!(
                "UPDATE memory.semantic_embedders
                    SET copy_valid_from = $2, copy_verify = $3
                  WHERE id = $1 AND status = 'current'",
                id,
                from,
                verify,
            )
            .execute(&self.pool)
            .await?;
            Ok(r.rows_affected() > 0)
        })
        .await
    }

    pub async fn semantic_verify_sample(
        self,
        since: DateTime<Utc>,
        model: &str,
        seed: &str,
        limit: i64,
    ) -> Result<Vec<VerifySample>, PersistError> {
        crate::metrics::time_db("semantic_verify_sample", async move {
            Ok(sqlx::query_as!(
                VerifySample,
                r#"SELECT c.id, c.chunk_text, c.chunk_embedding::text AS "embedding!"
                     FROM memory.operator_turn_chunks c
                    WHERE c.embedded_at >= $1 AND c.embedding_model = $2
                      AND char_length(c.chunk_text) > 0
                    ORDER BY md5(c.id::text || $3), c.id
                    LIMIT $4"#,
                since,
                model,
                seed,
                limit,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct PruneReport {
    pub candidates: u64,
    pub assignments: u64,
    pub items: u64,
    pub drift_rows: u64,
    pub floors: u64,
    pub retired: bool,
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn prune_semantic_embedder(
        self,
        embedder_key: &str,
    ) -> Result<Option<PruneReport>, PersistError> {
        crate::metrics::time_db("prune_semantic_embedder", async move {
            let mut tx = self.pool.begin().await?;
            let status = sqlx::query_scalar!(
                "SELECT status FROM memory.semantic_embedders WHERE embedder_key = $1 FOR UPDATE",
                embedder_key,
            )
            .fetch_optional(&mut *tx)
            .await?;
            if status.as_deref() != Some("retired") {
                return Ok(None);
            }
            let candidates = sqlx::query!(
                "DELETE FROM memory.pattern_candidates
                  WHERE embedder_key = $1 AND status IN ('open', 'superseded')",
                embedder_key,
            )
            .execute(&mut *tx)
            .await?
            .rows_affected();
            let assignments = sqlx::query!(
                "DELETE FROM memory.pattern_assignments WHERE embedder_key = $1",
                embedder_key,
            )
            .execute(&mut *tx)
            .await?
            .rows_affected();
            let items = sqlx::query!(
                "DELETE FROM memory.semantic_items WHERE embedder_key = $1",
                embedder_key,
            )
            .execute(&mut *tx)
            .await?
            .rows_affected();
            let drift_rows = sqlx::query!(
                "DELETE FROM eval.embedding_drift WHERE embedder_key = $1",
                embedder_key,
            )
            .execute(&mut *tx)
            .await?
            .rows_affected();
            let floors = sqlx::query!(
                "DELETE FROM memory.semantic_floors WHERE embedder_key = $1",
                embedder_key,
            )
            .execute(&mut *tx)
            .await?
            .rows_affected();
            tx.commit().await?;
            Ok(Some(PruneReport {
                candidates,
                assignments,
                items,
                drift_rows,
                floors,
                retired: true,
            }))
        })
        .await
    }
}
