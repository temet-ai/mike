use chrono::{DateTime, Utc};
use serde::Serialize;

use crate::analysis::embeddings::{EmbeddingConfig, EmbeddingService};
use crate::store::persistence::PersistHandle;
use crate::store::persistence::semantic::{EmbedderRow, NewEmbedder, parse_vector};

use super::embedder::{
    CANARY_VERSION, CanaryVectors, EmbedderShape, PREFIX_VERSION, VERIFY_SAMPLE, canary_texts,
    cosine, embedder_key, same_embedder, verify_passes,
};
use super::{SemanticError, egress, embed_error};

pub async fn embed_checked(
    embedding: &EmbeddingService,
    texts: &[String],
    allow_remote: bool,
) -> Result<(EmbeddingConfig, Vec<Vec<f32>>), SemanticError> {
    embedding
        .embed_documents_checked(texts, |c| egress::allow(c, allow_remote))
        .await
        .map_err(embed_error)
}

fn stored_canaries(row: &EmbedderRow) -> Option<CanaryVectors> {
    serde_json::from_value(row.canary_vectors.clone()).ok()
}

fn shape_matches(row: &EmbedderRow, config: &EmbeddingConfig, seen: &CanaryVectors) -> bool {
    let Some(stored) = stored_canaries(row) else {
        return false;
    };
    let dim = seen.vectors.first().map_or(0, Vec::len);
    same_embedder(
        &EmbedderShape {
            model: &row.model,
            dim: usize::try_from(row.dim).unwrap_or(0),
            chunk_max_chars: usize::try_from(row.chunk_max_chars).unwrap_or(0),
            prefix_version: &row.prefix_version,
            canaries: &stored,
        },
        &EmbedderShape {
            model: &config.model,
            dim,
            chunk_max_chars: config.chunk_max_chars,
            prefix_version: PREFIX_VERSION,
            canaries: seen,
        },
    )
}

pub async fn ensure_embedder(
    persist: &PersistHandle,
    embedding: &EmbeddingService,
    allow_remote: bool,
) -> Result<EmbedderRow, SemanticError> {
    let (config, vectors) = embed_checked(embedding, &canary_texts(), allow_remote).await?;
    let seen = CanaryVectors::new(vectors);
    if let Some(current) = persist.current_semantic_embedder().await?
        && shape_matches(&current, &config, &seen)
    {
        persist
            .touch_semantic_embedder(current.id, &config.url)
            .await?;
        return Ok(current);
    }
    let dim = seen.vectors.first().map_or(0, Vec::len);
    let now = Utc::now();
    let key = embedder_key(&config.model, dim, config.chunk_max_chars, now);
    persist
        .insert_semantic_embedder(&NewEmbedder {
            embedder_key: &key,
            model: &config.model,
            dim: i32::try_from(dim).unwrap_or(0),
            chunk_max_chars: i32::try_from(config.chunk_max_chars).unwrap_or(0),
            prefix_version: PREFIX_VERSION,
            url: &config.url,
            canary_version: CANARY_VERSION,
            canary_vectors: serde_json::to_value(&seen)
                .map_err(crate::store::persistence::PersistError::from)?,
            first_seen_at: now,
        })
        .await?;
    persist
        .current_semantic_embedder()
        .await?
        .ok_or(SemanticError::NoEmbedder)
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct VerifyReport {
    pub embedder_key: String,
    pub since: DateTime<Utc>,
    pub sample_ids: Vec<i64>,
    pub cosines: Vec<f64>,
    pub min_cosine: Option<f64>,
    pub passed: bool,
    pub written: bool,
}

pub async fn verify_copy_window(
    persist: &PersistHandle,
    embedding: &EmbeddingService,
    since: DateTime<Utc>,
    execute: bool,
    allow_remote: bool,
) -> Result<VerifyReport, SemanticError> {
    let current = persist
        .current_semantic_embedder()
        .await?
        .ok_or(SemanticError::NoEmbedder)?;
    let sample = persist
        .semantic_verify_sample(since, &current.model, "verify-v1", VERIFY_SAMPLE)
        .await?;
    let texts: Vec<String> = sample.iter().map(|s| s.chunk_text.clone()).collect();
    let (_, vectors) = embed_checked(embedding, &texts, allow_remote).await?;
    let cosines: Vec<f64> = sample
        .iter()
        .zip(&vectors)
        .map(|(s, v)| cosine(&parse_vector(&s.embedding), v))
        .collect();
    let min_cosine = cosines.iter().copied().reduce(f64::min);
    let passed = verify_passes(&cosines);
    let sample_ids: Vec<i64> = sample.iter().map(|s| s.id).collect();
    let written = if passed && execute {
        let verify = serde_json::json!({ "sample_ids": sample_ids, "min_cosine": min_cosine });
        persist
            .set_semantic_copy_window(current.id, since, &verify)
            .await?
    } else {
        false
    };
    Ok(VerifyReport {
        embedder_key: current.embedder_key,
        since,
        sample_ids,
        cosines,
        min_cosine,
        passed,
        written,
    })
}
