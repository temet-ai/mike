pub(crate) use super::knowledge_key::model_key;
use super::knowledge_key::{LEASE_ROLE, rekey_url_keyed_rows};
use super::knowledge_report::{KnowledgeSweepReport, SweepTally, publish};
use crate::analysis::{EmbeddingService, embeddings::chunk_text};
use crate::circuit_breaker::CircuitBreaker;
use crate::store::persistence::{
    PersistError, PersistHandle, project_knowledge::KnowledgeEmbeddingSource,
};
use serde_json::json;
use std::time::{Duration, Instant};

pub(crate) fn valid_vector(vector: &[f32], dim: usize) -> bool {
    vector.len() == dim && vector.iter().all(|v| v.is_finite()) && vector.iter().any(|v| *v != 0.0)
}

pub async fn sweep(
    persist: &PersistHandle,
    service: &EmbeddingService,
    breaker: &mut CircuitBreaker,
) -> Result<Option<KnowledgeSweepReport>, PersistError> {
    if !service.is_available() || breaker.should_skip(Instant::now()) {
        return Ok(None);
    }
    let model = model_key(service);
    let mut tally = SweepTally::default();
    sweep_rounds(persist, service, breaker, &model, &mut tally).await?;
    Ok(Some(tally.report(&model, chrono::Utc::now())))
}

async fn sweep_rounds(
    persist: &PersistHandle,
    service: &EmbeddingService,
    breaker: &mut CircuitBreaker,
    model: &str,
    tally: &mut SweepTally,
) -> Result<(), PersistError> {
    let started = Instant::now();
    for _ in 0..8 {
        let sources = persist.pending_knowledge_embeddings(model).await?;
        tally.read(sources.len());
        if sources.is_empty() {
            break;
        }
        for (attempted, source) in sources.iter().enumerate() {
            if breaker.should_skip(Instant::now()) || started.elapsed() >= Duration::from_secs(10) {
                tally.stopped_with(sources.len() - attempted);
                return Ok(());
            }
            embed_one(persist, service, breaker, source, model, tally).await?;
        }
    }
    Ok(())
}

async fn embed_one(
    persist: &PersistHandle,
    service: &EmbeddingService,
    breaker: &mut CircuitBreaker,
    source: &KnowledgeEmbeddingSource,
    model: &str,
    tally: &mut SweepTally,
) -> Result<(), PersistError> {
    match embed_source(persist, service, source, model).await {
        Ok(()) => {
            breaker.record(true, Instant::now());
            tally.embedded();
        }
        Err(error) => {
            breaker.record(!error.starts_with("[transient]"), Instant::now());
            persist
                .fail_knowledge_embedding(source, model, &error)
                .await?;
            tally.failed();
            tracing::warn!(reference = %source.reference, %error, "knowledge embedding failed; persisted exponential backoff");
        }
    }
    Ok(())
}

async fn embed_source(
    persist: &PersistHandle,
    service: &EmbeddingService,
    source: &KnowledgeEmbeddingSource,
    model: &str,
) -> Result<(), String> {
    let texts = chunk_text(&source.body, service.chunk_max_chars());
    let mut records = Vec::new();
    for batch in texts.chunks(16) {
        let vectors = tokio::time::timeout(Duration::from_secs(30), service.embed_documents(batch))
            .await
            .map_err(|_| "[transient] embedding request timed out".to_string())?
            .map_err(classify_embedding_error)?;
        if model_key(service) != model {
            return Err("[transient] Embedding configuration changed during collection".into());
        }
        for (text, vector) in batch.iter().zip(vectors) {
            if !valid_vector(&vector, service.dim()) {
                return Err("[permanent] Invalid embedding vector".into());
            }
            records.push(json!({"ordinal":records.len(),"body":text,
                "embedding":crate::store::operator_turns::vector_to_text_literal(&vector)}));
        }
    }
    persist
        .save_knowledge_embeddings(source, model, json!(records))
        .await
        .map_err(|e| format!("[transient] {e}"))
}

const SWEEP_INTERVAL: Duration = Duration::from_secs(30);

pub(crate) async fn sweep_if_leader(
    persist: &PersistHandle,
    service: &EmbeddingService,
    breaker: &mut CircuitBreaker,
) -> Result<bool, PersistError> {
    if !service.is_available() || !persist.holds_worker_role(LEASE_ROLE, SWEEP_INTERVAL).await {
        return Ok(false);
    }
    if let Some(report) = sweep(persist, service, breaker).await? {
        publish(report);
    }
    Ok(true)
}

async fn rekey_at_start(persist: &PersistHandle, service: &EmbeddingService) {
    match rekey_url_keyed_rows(persist, service).await {
        Ok(rows) => log_rekeyed(rows),
        Err(error) => tracing::warn!(%error, "knowledge embedding rekey failed"),
    }
}

fn log_rekeyed(rows: u64) {
    if rows > 0 {
        tracing::info!(
            rows,
            "knowledge embeddings moved to the host-independent key"
        );
    }
}

pub(crate) async fn run(
    persist: PersistHandle,
    service: EmbeddingService,
    shutdown: tokio_util::sync::CancellationToken,
) {
    rekey_at_start(&persist, &service).await;
    let mut interval = tokio::time::interval(SWEEP_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut breaker = CircuitBreaker::new(3, Duration::from_secs(30), Duration::from_secs(300));
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => return,
            _ = interval.tick() => {
                if let Err(error) = sweep_if_leader(&persist, &service, &mut breaker).await {
                    tracing::warn!(%error, "knowledge embedding sweep failed");
                }
            }
        }
    }
}

fn classify_embedding_error(error: crate::analysis::EmbedError) -> String {
    use crate::analysis::EmbedError;
    let permanent = match &error {
        EmbedError::Http(message) => [
            "HTTP 400", "HTTP 401", "HTTP 403", "HTTP 404", "HTTP 413", "HTTP 422",
        ]
        .iter()
        .any(|status| message.starts_with(status)),
        EmbedError::Refused(_) => true,
        _ => false,
    };
    let category = if permanent { "permanent" } else { "transient" };
    format!("[{category}] {error}")
}

#[cfg(test)]
mod retry_tests {
    use super::classify_embedding_error;
    use crate::analysis::EmbedError;

    #[test]
    fn transport_outages_remain_retryable_and_invalid_requests_are_terminal() {
        assert!(
            classify_embedding_error(EmbedError::Http("HTTP 503: unavailable".into()))
                .starts_with("[transient]")
        );
        assert!(classify_embedding_error(EmbedError::Disabled).starts_with("[transient]"));
        assert!(
            classify_embedding_error(EmbedError::Http("HTTP 400: invalid input".into()))
                .starts_with("[permanent]")
        );
    }
}
