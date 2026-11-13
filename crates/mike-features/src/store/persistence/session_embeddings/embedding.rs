use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::super::PersistError;
use crate::store::persistence::operator_turns::vector_to_text_literal;

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionEmbeddingCandidate {
    pub session_id: Uuid,
    pub overview: Option<String>,
    pub overview_generated_at: Option<DateTime<Utc>>,
    pub max_chunk_embedded_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, sqlx::FromRow)]
pub struct SessionEmbeddingStatus {
    pub source: String,
    pub model: String,
    pub embedded_at: DateTime<Utc>,
    pub chunk_count: i32,
}

pub struct SessionEmbeddingWrite<'a> {
    pub session_id: Uuid,
    pub embedding: &'a [f32],
    pub model: &'a str,
    pub source: &'a str,
    pub chunk_count: i32,
    pub source_updated_at: DateTime<Utc>,
}

pub fn parse_vector_text(s: &str) -> Result<Vec<f32>, PersistError> {
    let inner = s.trim();
    let inner = inner
        .strip_prefix('[')
        .and_then(|s| s.strip_suffix(']'))
        .ok_or_else(|| {
            PersistError::DataInvariant(format!("vector text literal missing brackets: {s:?}"))
        })?;
    if inner.is_empty() {
        return Ok(Vec::new());
    }
    inner
        .split(',')
        .map(|part| {
            part.trim().parse::<f32>().map_err(|e| {
                PersistError::DataInvariant(format!(
                    "invalid vector component {part:?} in {s:?}: {e}"
                ))
            })
        })
        .collect()
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn list_sessions_needing_embedding(
        self,
        limit: i64,
    ) -> Result<Vec<SessionEmbeddingCandidate>, PersistError> {
        crate::metrics::time_db("list_sessions_needing_embedding", async move {
            let rows = sqlx::query_as!(
                SessionEmbeddingCandidate,
                r#"SELECT s.session_uuid          AS "session_id!",
                          s.overview               AS "overview?",
                          s.overview_generated_at   AS "overview_generated_at?",
                          m.chunks_embedded_at      AS "max_chunk_embedded_at?"
                     FROM memory.session_chunk_marks m
                     JOIN sessions.sessions s ON s.id = m.session_id
                     LEFT JOIN memory.session_embeddings se ON se.session_id = s.session_uuid
                    WHERE (se.session_id IS NULL
                           OR se.source_updated_at < GREATEST(
                                m.chunks_embedded_at,
                                COALESCE(s.overview_generated_at, '-infinity'::timestamptz)
                              ))
                      AND EXISTS (
                          SELECT 1
                            FROM memory.operator_turns ot
                            JOIN memory.operator_turn_chunks otc ON otc.turn_id = ot.id
                           WHERE ot.session_id = m.session_id
                      )
                    ORDER BY GREATEST(
                            m.chunks_embedded_at,
                            COALESCE(s.overview_generated_at, '-infinity'::timestamptz)
                          ) ASC
                    LIMIT $1"#,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows)
        })
        .await
    }

    pub async fn session_chunk_centroid(
        self,
        session_id: Uuid,
    ) -> Result<Option<(Vec<f32>, i64, DateTime<Utc>)>, PersistError> {
        crate::metrics::time_db("session_chunk_centroid", async move {
            let row = sqlx::query_file!(
                "src/store/persistence/session_embeddings/chunk_centroid.sql",
                session_id,
            )
            .fetch_one(&self.pool)
            .await?;
            let chunk_count = row.chunk_count;
            let max_embedded_at = row.max_embedded_at;
            let embedding_text = row.embedding_text;
            if chunk_count == 0 {
                return Ok(None);
            }
            let embedding_text = embedding_text.ok_or_else(|| {
                PersistError::DataInvariant(format!(
                    "session {session_id} has {chunk_count} embedded chunks but the centroid \
                     aggregate returned no vector text"
                ))
            })?;
            let max_embedded_at = max_embedded_at.ok_or_else(|| {
                PersistError::DataInvariant(format!(
                    "session {session_id} has {chunk_count} embedded chunks but no max \
                     embedded_at"
                ))
            })?;
            let embedding = parse_vector_text(&embedding_text)?;
            Ok(Some((embedding, chunk_count, max_embedded_at)))
        })
        .await
    }

    #[rustfmt::skip]
    pub async fn upsert_session_embedding(self,
        write: SessionEmbeddingWrite<'_>,
    ) -> Result<(), PersistError> {
        crate::metrics::time_db("upsert_session_embedding", async move {
            let vec_literal = vector_to_text_literal(write.embedding);
            sqlx::query_file!(
                "src/store/persistence/session_embeddings/upsert_session_embedding.sql",
                write.session_id,
                vec_literal,
                write.model,
                write.source,
                write.chunk_count,
                write.source_updated_at,
            )
            .execute(&self.pool)
            .await?;
            Ok(())
        })
        .await
    }

    pub async fn session_embedding_status(
        self,
        session_id: Uuid,
    ) -> Result<Option<SessionEmbeddingStatus>, PersistError> {
        crate::metrics::time_db("session_embedding_status", async move {
            let row = sqlx::query_as!(
                SessionEmbeddingStatus,
                r#"SELECT source,
                          embedding_model AS model,
                          embedded_at,
                          chunk_count
                     FROM memory.session_embeddings
                    WHERE session_id = $1"#,
                session_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row)
        })
        .await
    }
}

#[cfg(test)]
pub mod tests {
    use super::*;

    #[test]
    fn parse_vector_text_roundtrips_vector_to_text_literal() -> anyhow::Result<()> {
        let original = [1.0_f32, -2.5, 0.0, 3.25];
        let literal = vector_to_text_literal(&original);
        let parsed = parse_vector_text(&literal)?;
        assert_eq!(parsed, original.to_vec());
        Ok(())
    }

    #[test]
    fn parse_vector_text_handles_empty_vector() -> anyhow::Result<()> {
        assert_eq!(parse_vector_text("[]")?, Vec::<f32>::new());
        Ok(())
    }

    #[test]
    fn parse_vector_text_rejects_missing_brackets() {
        assert!(parse_vector_text("1,2,3").is_err());
    }

    #[test]
    fn parse_vector_text_rejects_malformed_component() {
        assert!(parse_vector_text("[1,not-a-number,3]").is_err());
    }
}
