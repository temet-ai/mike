use crate::store::persistence::launch_repo::LaunchPersist as _;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::base_state::BaseState;
use crate::feature_toggles::FeatureToggles;
use crate::feature_toggles::live_gate::wait_until_on;
use crate::store::persistence::seat_memory::{SeatMemoryMarks, SeatMemoryRefresh};
use crate::store::persistence::{PersistError, PersistHandle};

#[cfg(test)]
mod tests;

const LEASE_ROLE: &str = "seat_memory_index";
const TICK: Duration = Duration::from_secs(30);
const BACKSTOP: Duration = Duration::from_secs(30 * 60);

pub fn spawn_seat_memory_worker(state: &BaseState) {
    let Some(persist) = state.persist().cloned() else {
        log_not_spawned("no persistence");
        return;
    };
    let toggles = state.feature_toggle_overlay.clone();
    state.tracker.spawn(run(persist, toggles, state.shutdown()));
    log_spawned();
}

#[derive(Debug, Default)]
pub struct RefreshMark {
    generation: Option<i64>,
    marks: Option<SeatMemoryMarks>,
    swept_at: Option<Instant>,
}

impl RefreshMark {
    pub fn due(&self, generation: i64, now: Instant) -> bool {
        self.generation != Some(generation) || self.sweep_due(now)
    }

    pub fn since(&self, now: Instant) -> Option<SeatMemoryMarks> {
        if self.sweep_due(now) {
            None
        } else {
            self.marks
        }
    }

    pub fn record(&mut self, generation: i64, now: Instant, refresh: Refreshed) {
        self.generation = Some(generation);
        self.marks = Some(refresh.marks);
        if refresh.swept {
            self.swept_at = Some(now);
        }
    }

    fn sweep_due(&self, now: Instant) -> bool {
        self.marks.is_none()
            || self
                .swept_at
                .is_none_or(|at| now.duration_since(at) >= BACKSTOP)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct Refreshed {
    pub marks: SeatMemoryMarks,
    pub swept: bool,
}

async fn run(
    persist: PersistHandle,
    toggles: Arc<ArcSwap<FeatureToggles>>,
    shutdown: CancellationToken,
) {
    let mut mark = RefreshMark::default();
    while wait_until_on(&toggles, |t| t.seat_memory_index, &shutdown).await {
        if !work_while_on(&persist, &toggles, &shutdown, &mut mark).await {
            return;
        }
        mark = RefreshMark::default();
        log_disabled();
    }
}

async fn work_while_on(
    persist: &PersistHandle,
    toggles: &ArcSwap<FeatureToggles>,
    shutdown: &CancellationToken,
    mark: &mut RefreshMark,
) -> bool {
    while toggles.load().seat_memory_index {
        step(persist, mark).await;
        tokio::select! {
            biased;
            () = shutdown.cancelled() => return false,
            () = tokio::time::sleep(TICK) => {}
        }
    }
    true
}

async fn step(persist: &PersistHandle, mark: &mut RefreshMark) {
    if !persist.holds_worker_role(LEASE_ROLE, TICK).await {
        *mark = RefreshMark::default();
        return;
    }
    let generation = match persist.launch().seat_memory_generation().await {
        Ok(generation) => generation,
        Err(e) => return log_failed("generation read", &e),
    };
    let now = Instant::now();
    if mark.due(generation, now) {
        refresh(persist, mark, generation, now).await;
    }
}

async fn refresh(persist: &PersistHandle, mark: &mut RefreshMark, generation: i64, now: Instant) {
    let since = mark.since(now);
    match persist.launch().refresh_seat_memory_items(since).await {
        Ok(outcome) => {
            let refreshed = Refreshed {
                marks: outcome.marks,
                swept: since.is_none(),
            };
            mark.record(generation, now, refreshed);
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
    tracing::info!("seat_memory_index: disabled via feature toggle — paused until it is turned on");
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
