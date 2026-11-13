use std::time::Duration;

use tokio::time::MissedTickBehavior;
use tokio_util::sync::CancellationToken;

use crate::analysis::{EmbeddingService, RerankService};
use crate::state::AppState;
use crate::store::persistence::{PersistError, PersistHandle};

const SYNC_INTERVAL: Duration = Duration::from_secs(10);

pub fn apply_stored_rows(
    rows: &[(String, serde_json::Value)],
    embedding: &EmbeddingService,
    rerank: &RerankService,
) -> Vec<String> {
    rows.iter()
        .filter(|(key, value)| apply_stored_row(key, value, embedding, rerank))
        .map(|(key, _)| key.clone())
        .collect()
}

fn apply_stored_row(
    key: &str,
    value: &serde_json::Value,
    embedding: &EmbeddingService,
    rerank: &RerankService,
) -> bool {
    if key.starts_with("embedding.") {
        return embedding.apply_stored_setting(key, value);
    }
    if key.starts_with("rerank.") || key.starts_with("search.ranking.") {
        return rerank.apply_stored_setting(key, value);
    }
    false
}

pub async fn sync_running_services(
    persist: &PersistHandle,
    embedding: &EmbeddingService,
    rerank: &RerankService,
) -> Result<Vec<String>, PersistError> {
    let rows = persist.fetch_settings().await?;
    Ok(apply_stored_rows(&rows, embedding, rerank))
}

pub fn spawn_running_service_sync(state: &AppState) {
    let Some(persist) = state.persist.clone() else {
        return;
    };
    let embedding = state.embedding_service();
    let rerank = state.rerank_service();
    let shutdown = state.shutdown();
    state.tracker.spawn(async move {
        run(persist, embedding, rerank, shutdown).await;
    });
}

async fn run(
    persist: PersistHandle,
    embedding: EmbeddingService,
    rerank: RerankService,
    shutdown: CancellationToken,
) {
    let mut interval = tokio::time::interval(SYNC_INTERVAL);
    interval.set_missed_tick_behavior(MissedTickBehavior::Delay);
    loop {
        tokio::select! {
            biased;
            _ = shutdown.cancelled() => return,
            _ = interval.tick() => sync_and_log(&persist, &embedding, &rerank).await,
        }
    }
}

async fn sync_and_log(
    persist: &PersistHandle,
    embedding: &EmbeddingService,
    rerank: &RerankService,
) {
    match sync_running_services(persist, embedding, rerank).await {
        Ok(changed) => log_followed(&changed),
        Err(error) => log_sync_failed(&error),
    }
}

fn log_followed(changed: &[String]) {
    if !changed.is_empty() {
        tracing::info!(keys = ?changed, "settings: running services followed settings stored by another machine");
    }
}

fn log_sync_failed(error: &PersistError) {
    tracing::warn!(%error, "settings: could not read stored settings for the running services; will retry");
}
