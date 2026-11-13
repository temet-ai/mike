use crate::store::persistence::lessons_repo::LessonsPersist as _;
use std::time::{Duration, Instant};

use tokio_util::sync::CancellationToken;

use crate::analysis::EmbeddingService;
use crate::circuit_breaker::CircuitBreaker;
use crate::state::AppState;
use crate::store::operator_turns::embed_closed_pending;
use crate::store::persistence::PersistError;
use crate::store::persistence::PersistHandle;

pub mod knowledge;
pub mod knowledge_key;
pub mod knowledge_report;
pub mod knowledge_settings;
pub mod vectors;

pub use vectors::SessionVectorSweepOutcome;
pub use vectors::refresh_session_embedding;

#[cfg(test)]
pub mod knowledge_counts_tests;
#[cfg(test)]
pub mod knowledge_floor_tests;
#[cfg(test)]
pub mod knowledge_header_tests;
#[cfg(test)]
pub mod knowledge_key_tests;
#[cfg(test)]
pub mod knowledge_report_tests;
#[cfg(test)]
pub mod knowledge_session_tests;
#[cfg(test)]
pub mod knowledge_tests;
#[cfg(test)]
pub mod knowledge_version_tests;
#[cfg(test)]
pub mod model_change_tests;

const IDLE_TURN_THRESHOLD: Duration = Duration::from_secs(30 * 60);

#[cfg(any(test, feature = "test-seams"))]
pub async fn run_drain_once(state: &AppState) -> anyhow::Result<usize> {
    let embedding = state.embedding_service();
    let Some(persist) = state.persist.clone() else {
        return Ok(0);
    };
    Ok(embed_closed_pending(&persist, &embedding).await?.written)
}

#[cfg(any(test, feature = "test-seams"))]
pub async fn run_reap_once(state: &AppState) -> anyhow::Result<Vec<i64>> {
    let Some(persist) = state.persist.clone() else {
        return Ok(Vec::new());
    };
    Ok(persist
        .reap_idle_open_turns(IDLE_TURN_THRESHOLD, chrono::Utc::now())
        .await?)
}

#[cfg(any(test, feature = "test-seams"))]
pub async fn run_session_vector_sweep_once(
    state: &AppState,
) -> anyhow::Result<Option<SessionVectorSweepOutcome>> {
    let embedding = state.embedding_service();
    let Some(persist) = state.persist.clone() else {
        return Ok(None);
    };
    Ok(Some(
        vectors::refresh_session_vectors(&persist, &embedding).await?,
    ))
}

const TURN_INTERVAL: Duration = Duration::from_secs(30);
const CATCHUP_TIME: Duration = Duration::from_secs(10);
const IDLE_BATCHES: usize = 8;

const BACKFILL_INTERVAL: Duration = Duration::from_secs(300);

const CIRCUIT_BREAKER_THRESHOLD: u32 = 3;

const CIRCUIT_BREAKER_COOLDOWN_BASE: Duration = Duration::from_secs(300);

const CIRCUIT_BREAKER_COOLDOWN_MAX: Duration = Duration::from_secs(3600);

pub fn spawn_embed_backfill(state: &AppState) {
    let embedding = state.embedding_service();
    let Some(persist) = state.persist.clone() else {
        return;
    };
    let shutdown = state.shutdown();

    let knowledge_persist = persist.clone();
    let knowledge_embedding = embedding.clone();
    let knowledge_shutdown = shutdown.clone();
    state.tracker.spawn(async move {
        knowledge::run(knowledge_persist, knowledge_embedding, knowledge_shutdown).await;
    });
    state.tracker.spawn(async move {
        run(persist, embedding, shutdown).await;
    });
}

async fn run(persist: PersistHandle, embedding: EmbeddingService, shutdown: CancellationToken) {
    log_starting();

    let mut interval = tokio::time::interval(BACKFILL_INTERVAL);
    interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);

    let mut turns = tokio::time::interval(TURN_INTERVAL);
    turns.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
    let mut breaker = new_breaker();
    let mut requeued_for: Option<String> = None;

    loop {
        tokio::select! {
            biased;

            _ = shutdown.cancelled() => {
                log_shutdown_signalled();
                return;
            }

            _ = interval.tick() => {
                run_one_tick(&persist, &embedding, &mut breaker).await;
            }

            _ = turns.tick() => {
                requeue_on_model_change(&persist, &embedding, &mut requeued_for).await;
                drain_catchup(&persist, &embedding, &mut breaker, &shutdown).await;
            }
        }
    }
}

pub async fn requeue_on_model_change(
    persist: &PersistHandle,
    embedding: &EmbeddingService,
    requeued_for: &mut Option<String>,
) {
    if !embedding.is_available() {
        return;
    }
    let model = embedding.model_name();
    if requeued_for.as_deref() == Some(model.as_str()) {
        return;
    }
    match persist
        .requeue_turns_embedded_with_other_model(&model)
        .await
    {
        Ok(turns) => {
            log_requeued(turns, &model);
            requeue_turns_without_chunks(persist).await;
            *requeued_for = Some(model);
        }
        Err(error) => log_requeue_failed(&error),
    }
}

async fn requeue_turns_without_chunks(persist: &PersistHandle) {
    match persist.requeue_turns_marked_embedded_without_chunks().await {
        Ok(turns) => log_requeued_without_chunks(turns),
        Err(error) => log_requeue_without_chunks_failed(&error),
    }
}

fn log_requeued_without_chunks(turns: u64) {
    if turns > 0 {
        tracing::info!(
            turns,
            "embed_backfill: turns marked embedded without chunks queued for embedding"
        );
    }
}

fn log_requeue_without_chunks_failed(error: &PersistError) {
    tracing::warn!(%error, "embed_backfill: queueing turns without chunks failed");
}

fn log_requeued(turns: u64, model: &str) {
    if turns > 0 {
        tracing::info!(turns, model = %model, "embed_backfill: turns embedded with another model queued for re-embedding");
    }
}

fn log_requeue_failed(error: &PersistError) {
    tracing::warn!(%error, "embed_backfill: re-embedding queue after a model change failed; will retry next tick");
}

fn log_starting() {
    tracing::debug!(
        "embed_backfill: starting turn drain ({}s) and vector sweep ({}s)",
        TURN_INTERVAL.as_secs(),
        BACKFILL_INTERVAL.as_secs()
    );
}

fn log_shutdown_signalled() {
    tracing::debug!("embed_backfill: shutdown signalled, exiting");
}

async fn run_one_tick(
    persist: &PersistHandle,
    embedding: &EmbeddingService,
    breaker: &mut CircuitBreaker,
) {
    let now = Instant::now();

    reap_idle_turns(persist).await;

    if breaker.should_skip(now) {
        log_breaker_open();
        return;
    }
    if breaker.is_probing(now) {
        log_breaker_probing();
    }

    if breaker.should_skip(Instant::now()) {
        log_vector_refresh_skipped();
        return;
    }
    refresh_vectors_and_log(persist, embedding, breaker).await;
}

async fn reap_idle_turns(persist: &PersistHandle) {
    match persist
        .reap_idle_open_turns(IDLE_TURN_THRESHOLD, chrono::Utc::now())
        .await
    {
        Ok(reaped) if !reaped.is_empty() => log_reaped(reaped.len()),
        Ok(_) => {}
        Err(e) => log_reap_failed(&e),
    }
}

fn log_reaped(count: usize) {
    tracing::info!(
        turns = count,
        "embed_backfill: reaped {} idle open turn(s)",
        count
    );
}

fn log_reap_failed(e: &PersistError) {
    tracing::warn!(error = %e, "embed_backfill: idle-turn reap failed — will retry next interval");
}

fn log_breaker_open() {
    tracing::debug!("embed_backfill: circuit breaker open — skipping sweep");
}

fn log_breaker_probing() {
    tracing::info!("embed_backfill: circuit breaker probing recovery");
}

async fn embed_pending_turns(
    persist: &PersistHandle,
    embedding: &EmbeddingService,
    breaker: &mut CircuitBreaker,
) -> usize {
    match embed_closed_pending(persist, embedding).await {
        Ok(outcome) => {
            breaker.record(!outcome.call_failed, Instant::now());
            if outcome.written > 0 {
                log_embedded(outcome.written);
            }
            outcome.written
        }
        Err(e) => {
            log_embed_sweep_failed(&e);
            0
        }
    }
}

fn log_embedded(count: usize) {
    tracing::info!(
        turns = count,
        "embed_backfill: embedded {} missed turn(s)",
        count
    );
}

fn log_embed_sweep_failed(e: &PersistError) {
    tracing::warn!(error = %e, "embed_backfill: sweep error — will retry next interval");
}

fn log_vector_refresh_skipped() {
    tracing::debug!(
        "embed_backfill: circuit breaker open — skipping session vector refresh this tick"
    );
}

async fn refresh_vectors_and_log(
    persist: &PersistHandle,
    embedding: &EmbeddingService,
    breaker: &mut CircuitBreaker,
) {
    match vectors::refresh_session_vectors(persist, embedding).await {
        Ok(outcome) => {
            breaker.record(!outcome.any_call_failed, Instant::now());
            if outcome.overview_written > 0
                || outcome.centroid_written > 0
                || outcome.projects_refreshed > 0
            {
                log_vectors_refreshed(&outcome);
            }
        }
        Err(e) => log_vector_sweep_failed(&e),
    }
}

fn log_vectors_refreshed(outcome: &SessionVectorSweepOutcome) {
    tracing::info!(
        overview = outcome.overview_written,
        centroid = outcome.centroid_written,
        failed = outcome.failed,
        projects = outcome.projects_refreshed,
        "embed_backfill: refreshed {} session vector(s) ({} overview, \
         {} centroid), {} project centroid(s)",
        outcome.overview_written + outcome.centroid_written,
        outcome.overview_written,
        outcome.centroid_written,
        outcome.projects_refreshed
    );
}

fn log_vector_sweep_failed(e: &PersistError) {
    tracing::warn!(
        error = %e,
        "embed_backfill: session vector sweep error — will retry next interval"
    );
}

fn new_breaker() -> CircuitBreaker {
    CircuitBreaker::new(
        CIRCUIT_BREAKER_THRESHOLD,
        CIRCUIT_BREAKER_COOLDOWN_BASE,
        CIRCUIT_BREAKER_COOLDOWN_MAX,
    )
}

async fn catchup_batches(persist: &PersistHandle) -> usize {
    let since = chrono::Utc::now() - chrono::Duration::minutes(5);
    match persist.lessons().has_recent_primary_activity(since).await {
        Ok(false) => IDLE_BATCHES,
        Ok(true) => 1,
        Err(error) => {
            tracing::warn!(%error, "embed_backfill: activity check failed, limiting drain to one batch");
            1
        }
    }
}

async fn drain_catchup(
    persist: &PersistHandle,
    embedding: &EmbeddingService,
    breaker: &mut CircuitBreaker,
    shutdown: &CancellationToken,
) -> usize {
    let started = Instant::now();
    if breaker.should_skip(started) {
        log_breaker_open();
        return 0;
    }
    let batches = catchup_batches(persist).await;
    let mut total = 0;
    for _ in 0..batches {
        if shutdown.is_cancelled() || started.elapsed() >= CATCHUP_TIME {
            break;
        }
        let written = embed_pending_turns(persist, embedding, breaker).await;
        total += written;
        if written == 0 || breaker.should_skip(Instant::now()) {
            break;
        }
    }
    total
}

#[cfg(any(test, feature = "test-seams"))]
pub async fn run_catchup_once(state: &AppState) -> usize {
    let Some(persist) = state.persist.as_ref() else {
        return 0;
    };
    drain_catchup(
        persist,
        &state.embedding_service(),
        &mut new_breaker(),
        &state.shutdown(),
    )
    .await
}
