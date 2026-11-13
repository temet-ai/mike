use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use chrono::{DateTime, Utc};
use uuid::Uuid;

use crate::analysis::{EmbedError, EmbeddingService};
use crate::store::persistence::session_embeddings::SessionEmbeddingWrite;
use crate::store::persistence::{PersistError, PersistHandle};

enum SessionEmbeddingRefresh {
    Overview,
    Centroid,
    NoSource,
    EmbedCallFailed,
}

async fn refresh_session_embedding_inner(
    persist: &PersistHandle,
    embedding_service: &EmbeddingService,
    session_id: Uuid,
    overview: Option<&str>,
    overview_generated_at: Option<DateTime<Utc>>,
) -> Result<SessionEmbeddingRefresh, PersistError> {
    let chunk_centroid = persist
        .knowledge()
        .session_chunk_centroid(session_id)
        .await?;

    if let Some(overview_text) = overview {
        let chunk_count = chunk_centroid
            .as_ref()
            .map_or(0, |(_, count, _)| i32::try_from(*count).unwrap_or(i32::MAX));
        let vectors = match embedding_service
            .embed_documents(&[overview_text.to_string()])
            .await
        {
            Ok(v) => v,
            Err(e) => {
                log_overview_embed_call_failed(session_id, &e);
                return Ok(SessionEmbeddingRefresh::EmbedCallFailed);
            }
        };
        let Some(vector) = vectors.into_iter().next() else {
            log_overview_embed_returned_no_vector(session_id);
            return Ok(SessionEmbeddingRefresh::EmbedCallFailed);
        };
        let model = embedding_service.model_name();
        let newest_chunk_at = chunk_centroid.as_ref().map(|(_, _, at)| *at);
        let source_updated_at = overview_generated_at
            .max(newest_chunk_at)
            .unwrap_or_else(Utc::now);
        persist
            .knowledge()
            .upsert_session_embedding(SessionEmbeddingWrite {
                session_id,
                embedding: &vector,
                model: &model,
                source: "overview",
                chunk_count,
                source_updated_at,
            })
            .await?;
        return Ok(SessionEmbeddingRefresh::Overview);
    }

    let Some((embedding, chunk_count, max_embedded_at)) = chunk_centroid else {
        return Ok(SessionEmbeddingRefresh::NoSource);
    };
    let model = embedding_service.model_name();
    let chunk_count_i32 = i32::try_from(chunk_count).unwrap_or(i32::MAX);
    persist
        .knowledge()
        .upsert_session_embedding(SessionEmbeddingWrite {
            session_id,
            embedding: &embedding,
            model: &model,
            source: "centroid",
            chunk_count: chunk_count_i32,
            source_updated_at: max_embedded_at,
        })
        .await?;
    Ok(SessionEmbeddingRefresh::Centroid)
}

fn log_overview_embed_call_failed(session_id: Uuid, e: &EmbedError) {
    tracing::warn!(
        session_id = %session_id,
        error = %e,
        "refresh_session_embedding: overview embed call failed"
    );
}

fn log_overview_embed_returned_no_vector(session_id: Uuid) {
    tracing::warn!(
        session_id = %session_id,
        "refresh_session_embedding: embed_documents returned no vector for the overview"
    );
}

pub async fn refresh_session_embedding(
    persist: &PersistHandle,
    embedding_service: &EmbeddingService,
    session_id: Uuid,
) -> Result<(), PersistError> {
    let overview = match persist
        .resolve_session_db_id(&session_id.to_string())
        .await?
    {
        Some(db_id) => persist.fetch_session_overview_text(db_id).await?,
        None => None,
    };
    let overview_text = overview
        .as_ref()
        .map(|(text, _)| text.as_str())
        .filter(|t| !t.trim().is_empty());
    let overview_generated_at = overview
        .as_ref()
        .and_then(|(_, generated_at)| *generated_at);
    refresh_session_embedding_inner(
        persist,
        embedding_service,
        session_id,
        overview_text,
        overview_generated_at,
    )
    .await?;
    Ok(())
}

#[derive(Debug, Default)]
pub struct SessionVectorSweepOutcome {
    pub overview_written: usize,
    pub centroid_written: usize,
    pub failed: usize,
    pub projects_refreshed: usize,
    pub any_call_failed: bool,
}

pub async fn refresh_session_vectors(
    persist: &PersistHandle,
    embedding_service: &EmbeddingService,
) -> Result<SessionVectorSweepOutcome, PersistError> {
    if !embedding_service.is_available() {
        return Ok(SessionVectorSweepOutcome::default());
    }
    const BATCH: i64 = 64;

    let candidates = persist
        .knowledge()
        .list_sessions_needing_embedding(BATCH)
        .await?;
    let mut outcome = SessionVectorSweepOutcome::default();
    for candidate in &candidates {
        let overview_text = candidate
            .overview
            .as_deref()
            .filter(|t| !t.trim().is_empty());
        match refresh_session_embedding_inner(
            persist,
            embedding_service,
            candidate.session_id,
            overview_text,
            candidate.overview_generated_at,
        )
        .await
        {
            Ok(SessionEmbeddingRefresh::Overview) => outcome.overview_written += 1,
            Ok(SessionEmbeddingRefresh::Centroid) => outcome.centroid_written += 1,
            Ok(SessionEmbeddingRefresh::NoSource) => {}
            Ok(SessionEmbeddingRefresh::EmbedCallFailed) => {
                outcome.failed += 1;
                outcome.any_call_failed = true;
            }
            Err(e) => {
                log_session_vector_refresh_failed(candidate.session_id, &e);
                outcome.failed += 1;
            }
        }
    }

    match persist.knowledge().refresh_project_centroids().await {
        Ok(n) => outcome.projects_refreshed = n,
        Err(e) => log_refresh_project_centroids_failed(&e),
    }

    Ok(outcome)
}

fn log_session_vector_refresh_failed(session_id: Uuid, e: &PersistError) {
    tracing::warn!(
        session_id = %session_id,
        error = %e,
        "embed_backfill: session vector refresh failed"
    );
}

fn log_refresh_project_centroids_failed(e: &PersistError) {
    tracing::warn!(error = %e, "embed_backfill: refresh_project_centroids failed");
}
