use super::knowledge_key::{LEASE_ROLE, rekey_url_keyed_rows};
pub use super::knowledge_key::{model_key, model_keys};
use super::knowledge_report::{KnowledgeSweepReport, SweepTally, publish};
use super::knowledge_settings::KnowledgeSweepSettings;
use crate::analysis::{EmbeddingService, embeddings::chunk_text};
use crate::circuit_breaker::CircuitBreaker;
use crate::store::persistence::knowledge_repo::KnowledgePersist as _;
use crate::store::persistence::{
    PersistError, PersistHandle, project_knowledge::KnowledgeEmbeddingSource,
};
use serde_json::json;
use std::time::{Duration, Instant};

pub fn valid_vector(vector: &[f32], dim: usize) -> bool {
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
    let settings = KnowledgeSweepSettings::load(persist).await;
    let mut tally = SweepTally::default();
    sweep_rounds(persist, service, breaker, &model, &mut tally, &settings).await?;
    Ok(Some(tally.report(&model, chrono::Utc::now())))
}

async fn sweep_rounds(
    persist: &PersistHandle,
    service: &EmbeddingService,
    breaker: &mut CircuitBreaker,
    model: &str,
    tally: &mut SweepTally,
    settings: &KnowledgeSweepSettings,
) -> Result<(), PersistError> {
    let started = Instant::now();
    while started.elapsed() < settings.budget {
        let sources = persist
            .knowledge()
            .pending_knowledge_embeddings_limited(model, settings.batch_sources)
            .await?;
        tally.read(sources.len());
        if sources.is_empty() {
            break;
        }
        for (index, window) in sources.chunks(settings.concurrency).enumerate() {
            if breaker.should_skip(Instant::now()) || started.elapsed() >= settings.budget {
                tally.stopped_with(sources.len() - index * settings.concurrency);
                return Ok(());
            }
            let results = futures::future::join_all(window.iter().map(|source| {
                embed_source(persist, service, source, model, settings.chunks_per_call)
            }))
            .await;
            for (source, result) in window.iter().zip(results) {
                record_outcome(persist, breaker, source, model, tally, result).await?;
            }
        }
    }
    Ok(())
}

async fn record_outcome(
    persist: &PersistHandle,
    breaker: &mut CircuitBreaker,
    source: &KnowledgeEmbeddingSource,
    model: &str,
    tally: &mut SweepTally,
    result: Result<(), String>,
) -> Result<(), PersistError> {
    match result {
        Ok(()) => {
            breaker.record(true, Instant::now());
            tally.embedded();
        }
        Err(error) => {
            breaker.record(!error.starts_with("[transient]"), Instant::now());
            persist
                .knowledge()
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
    chunks_per_call: usize,
) -> Result<(), String> {
    let texts = chunk_text(&source.body, service.chunk_max_chars());
    let mut records = Vec::new();
    for batch in texts.chunks(chunks_per_call) {
        let request = service.embed_titled_chunks(&source.header, batch);
        let vectors = tokio::time::timeout(Duration::from_secs(30), request)
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
        .knowledge()
        .save_knowledge_embeddings(source, model, json!(records))
        .await
        .map_err(|e| format!("[transient] {e}"))
}

pub async fn sweep_if_leader(
    persist: &PersistHandle,
    service: &EmbeddingService,
    breaker: &mut CircuitBreaker,
) -> Result<bool, PersistError> {
    if !service.is_available() {
        return Ok(false);
    }
    let interval = KnowledgeSweepSettings::load(persist).await.interval;
    if !persist.holds_worker_role(LEASE_ROLE, interval).await {
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

pub async fn run(
    persist: PersistHandle,
    service: EmbeddingService,
    shutdown: tokio_util::sync::CancellationToken,
) {
    rekey_at_start(&persist, &service).await;
    let mut breaker = CircuitBreaker::new(3, Duration::from_secs(30), Duration::from_secs(300));
    loop {
        let interval = KnowledgeSweepSettings::load(&persist).await.interval;
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => return,
            _ = tokio::time::sleep(interval) => {
                sweep_logging_errors(&persist, &service, &mut breaker).await;
            }
        }
    }
}

async fn sweep_logging_errors(
    persist: &PersistHandle,
    service: &EmbeddingService,
    breaker: &mut CircuitBreaker,
) {
    if let Err(error) = sweep_if_leader(persist, service, breaker).await {
        tracing::warn!(%error, "knowledge embedding sweep failed");
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
pub mod retry_tests {
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
