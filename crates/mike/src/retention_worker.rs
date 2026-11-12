use std::time::Duration;

use tokio::time::{MissedTickBehavior, interval};
use tokio_util::sync::CancellationToken;

use crate::config::AppState;
use crate::store::persistence::{PersistError, PersistHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct RetentionSweepOutcome {
    pub sessions_removed: u64,
    pub messages_removed: u64,
}

async fn run_retention_sweep_tick(
    persist: &PersistHandle,
    default_ttl: Duration,
    orphan_batch_size: i64,
) -> RetentionSweepOutcome {
    let now = chrono::Utc::now();
    let sessions_removed = match persist.sweep_expired_sessions(default_ttl, now).await {
        Ok(n) => n,
        Err(e) => {
            log_session_sweep_failed(&e);
            0
        }
    };

    let messages_removed = match persist.sweep_orphaned_messages(orphan_batch_size).await {
        Ok(n) => n,
        Err(e) => {
            log_orphan_sweep_failed(&e);
            0
        }
    };

    match persist
        .sweep_runtime_observations(now, orphan_batch_size)
        .await
    {
        Ok(outcome) => tracing::info!(?outcome, "runtime observation retention complete"),
        Err(error) => tracing::warn!(%error, "runtime observation retention failed"),
    }

    tracing::info!(
        sessions_removed,
        messages_removed,
        "retention_sweep: tick complete"
    );

    metrics::counter!(crate::metrics::COUNTER_RETENTION_SWEEP_TICKS_TOTAL).increment(1);
    metrics::gauge!(crate::metrics::GAUGE_RETENTION_SWEEP_LAST_RUN_UNIX_SECONDS)
        .set(now.timestamp() as f64);
    metrics::counter!(crate::metrics::COUNTER_RETENTION_SWEEP_SESSIONS_REMOVED_TOTAL)
        .increment(sessions_removed);
    metrics::counter!(crate::metrics::COUNTER_RETENTION_SWEEP_MESSAGES_REMOVED_TOTAL)
        .increment(messages_removed);

    RetentionSweepOutcome {
        sessions_removed,
        messages_removed,
    }
}

fn log_session_sweep_failed(e: &PersistError) {
    tracing::warn!(error = %e, "retention_sweep: session sweep failed");
}

fn log_orphan_sweep_failed(e: &PersistError) {
    tracing::warn!(error = %e, "retention_sweep: orphaned-message sweep failed");
}

pub fn spawn_retention_sweep_worker(state: &AppState) {
    if !state.feature_toggle_overlay.load().retention_sweep {
        log_not_spawning_toggle_off();
        return;
    }

    let Some(persist) = state.persist().cloned() else {
        log_not_spawning_no_persistence();
        return;
    };

    let cfg = state.config().retention_sweep.clone();
    let default_ttl =
        crate::store::retention::RetentionPolicy::parse_duration(&state.config().default_ttl);
    let toggles = state.feature_toggle_overlay.clone();
    let shutdown = state.shutdown();
    let tracker = state.tracker.clone();

    tracker.spawn(async move {
        run(persist, default_ttl, cfg, toggles, shutdown).await;
    });

    tracing::info!(
        interval_secs = state.config().retention_sweep.interval_secs,
        orphan_batch_size = state.config().retention_sweep.orphan_batch_size,
        default_ttl = ?default_ttl,
        "retention_sweep: spawned"
    );
}

fn log_not_spawning_toggle_off() {
    tracing::debug!("retention_sweep: disabled by feature toggle — not spawning");
}

fn log_not_spawning_no_persistence() {
    tracing::debug!("retention_sweep: no persistence — not spawning");
}

async fn run(
    persist: PersistHandle,
    default_ttl: Duration,
    cfg: crate::config::retention_sweep::RetentionSweepConfig,
    toggles: std::sync::Arc<arc_swap::ArcSwap<crate::feature_toggles::FeatureToggles>>,
    shutdown: CancellationToken,
) {
    let mut ticker = interval(Duration::from_secs(cfg.interval_secs));
    ticker.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            biased;

            _ = shutdown.cancelled() => {
                log_shutdown_signalled();
                return;
            }

            _ = ticker.tick() => {
                if !toggles.load().retention_sweep {
                    log_disabled_via_toggle();
                    return;
                }

                run_retention_sweep_tick(&persist, default_ttl, cfg.orphan_batch_size).await;
            }
        }
    }
}

fn log_shutdown_signalled() {
    tracing::debug!("retention_sweep: shutdown signalled, exiting");
}

fn log_disabled_via_toggle() {
    tracing::info!(
        "retention_sweep: disabled via feature toggle — exiting (a UI flip back on needs a restart)"
    );
}

#[cfg(any(test, feature = "test-seams"))]
pub async fn run_retention_sweep_once(state: &AppState) -> anyhow::Result<RetentionSweepOutcome> {
    let Some(persist) = state.persist() else {
        return Ok(RetentionSweepOutcome::default());
    };
    let default_ttl =
        crate::store::retention::RetentionPolicy::parse_duration(&state.config().default_ttl);
    let batch_size = state.config().retention_sweep.orphan_batch_size;
    Ok(run_retention_sweep_tick(persist, default_ttl, batch_size).await)
}
