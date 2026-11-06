pub mod link;
pub mod own_marker;
pub mod sweep;

use std::time::Duration;

use tokio::time::{MissedTickBehavior, interval};
use tokio_util::sync::CancellationToken;

use crate::config::AppState;
use crate::config::channel_link_sweep::ChannelLinkSweepConfig;
use crate::store::persistence::PersistHandle;

const CANDIDATE_WINDOW_MINUTES: i64 = 120;

pub fn spawn_channel_link_sweep(state: &AppState) {
    if !state.feature_toggle_overlay.load().channel_link_sweep {
        log_not_spawning_toggle_off();
        return;
    }

    let Some(persist) = state.persist().cloned() else {
        log_not_spawning_no_persistence();
        return;
    };

    let cfg = state.config().channel_link_sweep.clone();
    let toggles = state.feature_toggle_overlay.clone();
    let shutdown = state.shutdown();
    let tracker = state.tracker.clone();

    let interval_secs = cfg.interval_secs;
    let batch_size = cfg.batch_size;

    tracker.spawn(async move {
        run(persist, cfg, toggles, shutdown).await;
    });

    tracing::info!(
        interval_secs,
        batch_size,
        candidate_window_minutes = CANDIDATE_WINDOW_MINUTES,
        "channel_link_sweep: spawned"
    );
}

fn log_not_spawning_toggle_off() {
    tracing::debug!("channel_link_sweep: disabled by feature toggle — not spawning");
}

fn log_not_spawning_no_persistence() {
    tracing::debug!("channel_link_sweep: no persistence — not spawning");
}

async fn run(
    persist: PersistHandle,
    cfg: ChannelLinkSweepConfig,
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
                if !toggles.load().channel_link_sweep {
                    log_disabled_via_toggle();
                    return;
                }

                sweep::run_sweep_tick(&persist, CANDIDATE_WINDOW_MINUTES, cfg.batch_size).await;
            }
        }
    }
}

fn log_shutdown_signalled() {
    tracing::debug!("channel_link_sweep: shutdown signalled, exiting");
}

fn log_disabled_via_toggle() {
    tracing::info!(
        "channel_link_sweep: disabled via feature toggle — exiting (a UI flip back on needs a restart)"
    );
}
