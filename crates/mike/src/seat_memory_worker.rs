use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::feature_toggles::FeatureToggles;
use crate::state::AppState;
use crate::store::persistence::seat_memory::SeatMemoryRefresh;
use crate::store::persistence::{PersistError, PersistHandle};

#[cfg(test)]
mod tests;

const LEASE_ROLE: &str = "seat_memory_index";
const TICK: Duration = Duration::from_secs(30);
const BACKSTOP: Duration = Duration::from_secs(30 * 60);

pub fn spawn_seat_memory_worker(state: &AppState) {
    if !state.feature_toggle_overlay.load().seat_memory_index {
        log_not_spawned("disabled by feature toggle");
        return;
    }
    let Some(persist) = state.persist().cloned() else {
        log_not_spawned("no persistence");
        return;
    };
    let toggles = state.feature_toggle_overlay.clone();
    state.tracker.spawn(run(persist, toggles, state.shutdown()));
    log_spawned();
}

#[derive(Debug, Default)]
pub(crate) struct RefreshMark {
    generation: Option<i64>,
    refreshed_at: Option<Instant>,
}

impl RefreshMark {
    pub(crate) fn due(&self, generation: i64, now: Instant) -> bool {
        self.generation != Some(generation)
            || self
                .refreshed_at
                .is_none_or(|at| now.duration_since(at) >= BACKSTOP)
    }

    pub(crate) fn record(&mut self, generation: i64, now: Instant) {
        self.generation = Some(generation);
        self.refreshed_at = Some(now);
    }
}

async fn run(
    persist: PersistHandle,
    toggles: Arc<ArcSwap<FeatureToggles>>,
    shutdown: CancellationToken,
) {
    let mut mark = RefreshMark::default();
    while toggles.load().seat_memory_index {
        step(&persist, &mut mark).await;
        tokio::select! {
            biased;
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(TICK) => {}
        }
    }
    log_disabled();
}

async fn step(persist: &PersistHandle, mark: &mut RefreshMark) {
    if !persist.holds_worker_role(LEASE_ROLE, TICK).await {
        *mark = RefreshMark::default();
        return;
    }
    let generation = match persist.seat_memory_generation().await {
        Ok(generation) => generation,
        Err(e) => return log_failed("generation read", &e),
    };
    let now = Instant::now();
    if mark.due(generation, now) {
        refresh(persist, mark, generation, now).await;
    }
}

async fn refresh(persist: &PersistHandle, mark: &mut RefreshMark, generation: i64, now: Instant) {
    match persist.refresh_seat_memory_items().await {
        Ok(outcome) => {
            mark.record(generation, now);
            log_refreshed(outcome);
        }
        Err(e) => log_failed("refresh", &e),
    }
}

fn log_not_spawned(reason: &str) {
    tracing::debug!(reason, "seat_memory_index: not spawning");
}

fn log_spawned() {
    tracing::info!(
        tick_secs = TICK.as_secs(),
        "seat_memory_index: worker spawned"
    );
}

fn log_disabled() {
    tracing::info!(
        "seat_memory_index: disabled via feature toggle — exiting (a UI flip back on needs a restart)"
    );
}

fn log_failed(what: &str, e: &PersistError) {
    tracing::warn!(error = %e, what, "seat_memory_index: step failed");
}

fn log_refreshed(outcome: SeatMemoryRefresh) {
    tracing::info!(
        items = outcome.items,
        written = outcome.written,
        removed = outcome.removed,
        "seat_memory_index: refreshed"
    );
}
