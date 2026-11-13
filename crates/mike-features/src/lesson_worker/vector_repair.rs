use crate::store::persistence::lessons_repo::LessonsPersist as _;
use std::time::{Duration, Instant};

use crate::analysis::EmbeddingService;
use crate::circuit_breaker::CircuitBreaker;
use crate::store::persistence::PersistHandle;

pub struct EmbeddingWork {
    pub consolidation: CircuitBreaker,
    repair_breaker: CircuitBreaker,
    after_id: i64,
}

impl EmbeddingWork {
    pub fn new() -> Self {
        Self {
            consolidation: breaker(),
            repair_breaker: breaker(),
            after_id: 0,
        }
    }

    pub async fn repair(&mut self, persist: &PersistHandle, embedding: &EmbeddingService) {
        if !embedding.is_available() || self.repair_breaker.should_skip(Instant::now()) {
            return;
        }
        let model = embedding.model_name();
        let rows = match persist
            .lessons()
            .pending_lesson_vector_repairs(&model, self.after_id, 16)
            .await
        {
            Ok(rows) => rows,
            Err(error) => {
                tracing::warn!(%error, "lesson vector repair discovery failed");
                return;
            }
        };
        if rows.is_empty() {
            self.after_id = 0;
            return;
        }
        let started = Instant::now();
        for (id, text) in rows {
            if started.elapsed() >= Duration::from_secs(10) || !embedding.is_available() {
                break;
            }
            self.after_id = id;
            let success = repair_one(persist, embedding, id, &text).await;
            self.repair_breaker.record(success, Instant::now());
            if !success {
                break;
            }
        }
    }
}

fn breaker() -> CircuitBreaker {
    CircuitBreaker::new(
        super::CIRCUIT_BREAKER_THRESHOLD,
        Duration::from_secs(super::CIRCUIT_BREAKER_COOLDOWN_BASE_SECS),
        Duration::from_secs(super::CIRCUIT_BREAKER_COOLDOWN_MAX_SECS),
    )
}

async fn repair_one(
    persist: &PersistHandle,
    embedding: &EmbeddingService,
    id: i64,
    text: &str,
) -> bool {
    let Some((model, vector)) = embed_vector(embedding, id, text).await else {
        return false;
    };
    let result = persist
        .lessons()
        .write_repaired_lesson_vector(id, text, &vector, &model)
        .await;
    record_write_result(id, result)
}

async fn embed_vector(
    embedding: &EmbeddingService,
    id: i64,
    text: &str,
) -> Option<(String, Vec<f32>)> {
    let (config, mut vectors) = match embedding
        .embed_documents_checked(&[text.to_owned()], |_| true)
        .await
    {
        Ok(result) => result,
        Err(error) => {
            log_embedding_failure(id, &error);
            return None;
        }
    };
    let vector = vectors
        .pop()
        .filter(|vector| valid_vector(vector, embedding.dim()));
    if vector.is_none() {
        log_invalid_vector(id);
    }
    vector.map(|vector| (config.model, vector))
}

fn valid_vector(vector: &[f32], dimension: usize) -> bool {
    vector.len() == dimension && vector.iter().all(|value| value.is_finite())
}

fn record_write_result(
    id: i64,
    result: Result<bool, crate::store::persistence::PersistError>,
) -> bool {
    match result {
        Ok(written) => {
            tracing::debug!(id, written, "lesson vector repair finished");
            true
        }
        Err(error) => {
            log_write_failure(id, &error);
            false
        }
    }
}

fn log_embedding_failure(id: i64, error: &crate::analysis::EmbedError) {
    tracing::warn!(id, %error, "lesson vector repair embedding failed; will retry");
}

fn log_invalid_vector(id: i64) {
    tracing::warn!(
        id,
        "lesson vector repair received invalid dimensions or values"
    );
}

fn log_write_failure(id: i64, error: &crate::store::persistence::PersistError) {
    tracing::warn!(id, %error, "lesson vector repair persistence failed; will retry");
}
