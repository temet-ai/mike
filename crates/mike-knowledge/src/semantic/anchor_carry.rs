use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use chrono::Utc;

use crate::analysis::embeddings::EmbeddingService;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::semantic::{AnchorSource, EmbedderRow, NewItem};

use super::check::embed_checked;
use super::{BUILDER_VERSION, SemanticError};

const CARRY_BATCH: i64 = 64;

pub fn carried(source: &AnchorSource, embedder: &EmbedderRow, embedding: Vec<f32>) -> NewItem {
    NewItem {
        kind: "anchor",
        request_id: source.request_id,
        origin_session_uuid: source.origin_session_uuid.clone(),
        origin_turn_id: source.origin_turn_id,
        source_ref: source.source_ref.clone(),
        pattern: Some(source.pattern.clone()),
        text: source.text.clone(),
        embedding,
        embedder_key: embedder.embedder_key.clone(),
        vector_source: "embedded",
        builder_version: BUILDER_VERSION,
        effective_at: Utc::now(),
        data: source.data.clone(),
        ..NewItem::default()
    }
}

pub async fn carry_anchors(
    persist: &PersistHandle,
    embedding: &EmbeddingService,
    embedder: &EmbedderRow,
    allow_remote: bool,
) -> Result<u64, SemanticError> {
    let sources = persist
        .knowledge()
        .anchors_missing_from(&embedder.embedder_key, CARRY_BATCH)
        .await?;
    if sources.is_empty() {
        return Ok(0);
    }
    let texts: Vec<String> = sources.iter().map(|s| s.text.clone()).collect();
    let (_, vectors) = embed_checked(embedding, &texts, allow_remote).await?;
    let items: Vec<NewItem> = sources
        .iter()
        .zip(vectors)
        .map(|(source, vector)| carried(source, embedder, vector))
        .collect();
    Ok(persist.knowledge().insert_semantic_items(&items).await?)
}

#[cfg(test)]
#[path = "tests/anchor_carry_tests.rs"]
mod tests;
