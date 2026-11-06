use chrono::{DateTime, Utc};
use serde::Serialize;

use super::super::{PersistError, PersistHandle};

#[derive(Debug, Clone, PartialEq)]
pub struct AnchorVector {
    pub id: i64,
    pub pattern: String,
    pub origin_session_uuid: Option<String>,
    pub request_id: Option<i64>,
    pub embedding: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct TaggerBacktestCase {
    pub item_id: i64,
    pub patterns: Vec<String>,
    pub origin_session_uuid: Option<String>,
    pub request_id: Option<i64>,
    pub embedding: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct FacetVector {
    pub facet: String,
    pub vector: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ScopeFilter {
    pub scope: &'static str,
    pub project_id: Option<i64>,
    pub session_id: Option<i64>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct CandidateSearch<'a> {
    pub embedder_key: &'a str,
    pub facets: &'a [FacetVector],
    pub as_of: DateTime<Utc>,
    pub reconstructed: bool,
    pub audience: &'static str,
    pub scope: ScopeFilter,
    pub subject_turn: Option<i64>,
    pub limit: i64,
    pub embedding_model: &'a str,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CandidateRow {
    pub item_id: i64,
    pub item_facet: String,
    pub text: String,
    pub source_ref: String,
    pub operator_turn_id: Option<i64>,
    pub session_id: Option<i64>,
    pub project_id: Option<i64>,
    pub request_id: Option<i64>,
    pub effective_at: DateTime<Utc>,
    pub recorded_at: DateTime<Utc>,
    pub query_facet: String,
    pub similarity: f64,
    pub qualification_ref: Option<String>,
    pub qualification: Option<String>,
    pub corrected_turn_id: Option<i64>,
}

impl PersistHandle {
    pub async fn semantic_turns_with_items(
        &self,
        embedder_key: &str,
        from: DateTime<Utc>,
        to: DateTime<Utc>,
        limit: i64,
    ) -> Result<Vec<(i64, DateTime<Utc>)>, PersistError> {
        crate::metrics::time_db("semantic_turns_with_items", async move {
            let rows = sqlx::query!(
                r#"SELECT DISTINCT l.subject_id AS "subject_id!", t.closed_at AS "closed_at!"
                     FROM eval.labels_as_of('infinity'::timestamptz) l
                     JOIN memory.operator_turns t ON t.id = l.subject_id
                    WHERE l.subject_kind = 'turn' AND l.source = 'fleet_mirror/ground_truth'
                      AND l.split = 'dev' AND t.closed_at >= $2 AND t.closed_at < $3
                      AND EXISTS (
                          SELECT 1 FROM memory.semantic_items i
                           WHERE i.operator_turn_id = l.subject_id AND i.embedder_key = $1
                             AND i.kind IN ('turn_prompt', 'turn_ending'))
                    ORDER BY l.subject_id
                    LIMIT $4"#,
                embedder_key,
                from,
                to,
                limit,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|r| (r.subject_id, r.closed_at))
                .collect())
        })
        .await
    }

    pub async fn semantic_anchors(
        &self,
        embedder_key: &str,
    ) -> Result<Vec<AnchorVector>, PersistError> {
        crate::metrics::time_db("semantic_anchors", async move {
            Ok(sqlx::query_as!(
                AnchorVector,
                r#"SELECT id, pattern AS "pattern!",
                          origin_session_uuid::text AS origin_session_uuid, request_id,
                          embedding::text AS "embedding!"
                     FROM memory.semantic_items
                    WHERE kind = 'anchor' AND embedder_key = $1 AND pattern IS NOT NULL
                    ORDER BY id"#,
                embedder_key,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn semantic_tagger_backtest_cases(
        &self,
        embedder_key: &str,
    ) -> Result<Vec<TaggerBacktestCase>, PersistError> {
        crate::metrics::time_db("semantic_tagger_backtest_cases", async move {
            Ok(sqlx::query_as!(
                TaggerBacktestCase,
                r#"SELECT p.id AS item_id,
                          array_agg(DISTINCT a.pattern ORDER BY a.pattern) AS "patterns!",
                          a.origin_session_uuid::text AS origin_session_uuid, a.request_id,
                          p.embedding::text AS "embedding!"
                     FROM memory.semantic_items a
                     JOIN memory.semantic_items p ON p.kind = 'turn_prompt'
                      AND p.embedder_key = a.embedder_key AND p.request_id = a.request_id
                    WHERE a.kind = 'anchor' AND a.embedder_key = $1 AND a.pattern IS NOT NULL
                    GROUP BY p.id, a.origin_session_uuid, a.request_id, p.embedding
                    ORDER BY p.id"#,
                embedder_key,
            )
            .fetch_all(&self.pool)
            .await?)
        })
        .await
    }

    pub async fn semantic_session_project(
        &self,
        session_id: i64,
    ) -> Result<Option<i64>, PersistError> {
        crate::metrics::time_db("semantic_session_project", async move {
            let row = sqlx::query_scalar!(
                r#"SELECT project_id AS "project_id?" FROM sessions.sessions WHERE id = $1"#,
                session_id,
            )
            .fetch_optional(&self.pool)
            .await?;
            Ok(row.flatten())
        })
        .await
    }

    pub async fn semantic_subject_facets(
        &self,
        turn_id: i64,
        embedder_key: &str,
    ) -> Result<Vec<FacetVector>, PersistError> {
        crate::metrics::time_db("semantic_subject_facets", async move {
            let rows = sqlx::query!(
                r#"SELECT DISTINCT ON (i.kind) i.kind, i.embedding::text AS "vec!"
                     FROM memory.semantic_items i
                    WHERE i.operator_turn_id = $1 AND i.embedder_key = $2
                      AND i.kind IN ('turn_prompt', 'turn_ending')
                    ORDER BY i.kind, i.builder_version DESC, i.id DESC"#,
                turn_id,
                embedder_key,
            )
            .fetch_all(&self.pool)
            .await?;
            Ok(rows
                .into_iter()
                .map(|row| FacetVector {
                    facet: if row.kind == "turn_prompt" {
                        "prompt"
                    } else {
                        "ending"
                    }
                    .to_owned(),
                    vector: row.vec,
                })
                .collect())
        })
        .await
    }

    pub async fn semantic_candidates(
        &self,
        kind: &str,
        s: &CandidateSearch<'_>,
    ) -> Result<Vec<CandidateRow>, PersistError> {
        let facets: Vec<String> = s.facets.iter().map(|f| f.facet.clone()).collect();
        let vectors: Vec<String> = s.facets.iter().map(|f| f.vector.clone()).collect();
        if !matches!(kind, "correction" | "exchange" | "directive" | "lesson") {
            return Err(PersistError::DataInvariant(format!(
                "unknown retrieval kind {kind}"
            )));
        }
        crate::metrics::time_db("fetch_semantic_neighbours", async move {
            let mut tx = self.pool.begin().await?;
            sqlx::query!("SET LOCAL hnsw.iterative_scan = strict_order")
                .execute(&mut *tx)
                .await?;
            sqlx::query!("SET LOCAL hnsw.ef_search = 100")
                .execute(&mut *tx)
                .await?;
            let rows = neighbour_rows(&mut tx, kind, s, &facets, &vectors).await?;
            tx.commit().await?;
            Ok(rows)
        })
        .await
    }
}

async fn neighbour_rows(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    kind: &str,
    s: &CandidateSearch<'_>,
    facets: &[String],
    vectors: &[String],
) -> Result<Vec<CandidateRow>, PersistError> {
    match kind {
        "correction" => correction_rows(tx, s, facets, vectors).await,
        "exchange" => exchange_rows(tx, s, facets, vectors).await,
        "directive" => directive_rows(tx, s, facets, vectors).await,
        _ => lesson_rows(tx, s, facets, vectors).await,
    }
}

async fn correction_rows(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    s: &CandidateSearch<'_>,
    facets: &[String],
    vectors: &[String],
) -> Result<Vec<CandidateRow>, PersistError> {
    Ok(sqlx::query_file_as!(
        CandidateRow,
        "src/store/persistence/semantic/neighbours_correction.sql",
        facets,
        vectors,
        s.embedder_key,
        s.as_of,
        s.reconstructed,
        s.subject_turn,
        s.scope.scope,
        s.scope.project_id,
        s.scope.session_id,
        s.limit,
        s.audience,
    )
    .fetch_all(&mut **tx)
    .await?)
}

async fn exchange_rows(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    s: &CandidateSearch<'_>,
    facets: &[String],
    vectors: &[String],
) -> Result<Vec<CandidateRow>, PersistError> {
    Ok(sqlx::query_file_as!(
        CandidateRow,
        "src/store/persistence/semantic/neighbours_exchange.sql",
        facets,
        vectors,
        s.embedder_key,
        s.as_of,
        s.reconstructed,
        s.subject_turn,
        s.scope.scope,
        s.scope.project_id,
        s.scope.session_id,
        s.limit,
    )
    .fetch_all(&mut **tx)
    .await?)
}

async fn directive_rows(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    s: &CandidateSearch<'_>,
    facets: &[String],
    vectors: &[String],
) -> Result<Vec<CandidateRow>, PersistError> {
    Ok(sqlx::query_file_as!(
        CandidateRow,
        "src/store/persistence/semantic/neighbours_directive.sql",
        facets,
        vectors,
        s.embedder_key,
        s.as_of,
        s.reconstructed,
        s.subject_turn,
        s.scope.scope,
        s.scope.project_id,
        s.scope.session_id,
        s.limit,
    )
    .fetch_all(&mut **tx)
    .await?)
}

async fn lesson_rows(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    s: &CandidateSearch<'_>,
    facets: &[String],
    vectors: &[String],
) -> Result<Vec<CandidateRow>, PersistError> {
    Ok(sqlx::query_file_as!(
        CandidateRow,
        "src/store/persistence/semantic/neighbours_lesson.sql",
        facets,
        vectors,
        s.embedding_model,
        s.as_of,
        s.reconstructed,
        s.subject_turn,
        s.scope.scope,
        s.scope.project_id,
        s.scope.session_id,
        s.limit,
    )
    .fetch_all(&mut **tx)
    .await?)
}
