pub mod link;
pub mod local_registry;
pub mod own_marker;
pub mod sweep;

use std::time::Duration;

use tokio::time::{MissedTickBehavior, interval};
use tokio_util::sync::CancellationToken;

use crate::channel_link_settings::ChannelLinkSweepConfig;
use crate::feature_toggles::live_gate::wait_until_on;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;

const CANDIDATE_WINDOW_MINUTES: i64 = 120;

pub fn spawn_channel_link_sweep(state: &AppState) {
    let Some(persist) = state.persist().cloned() else {
        log_not_spawning_no_persistence();
        return;
    };

    let cfg = state.config().sections.get::<ChannelLinkSweepConfig>();
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
                if !wait_until_on(&toggles, |t| t.channel_link_sweep, &shutdown).await {
                    log_shutdown_signalled();
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
