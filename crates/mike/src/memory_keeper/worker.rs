use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use chrono::Utc;
use tokio_util::sync::CancellationToken;

use super::health::{self, LastPass};
use super::rebuild::{PassReport, merge, rebuild};
use super::refresh::refresh;
use super::snapshot::KeeperCache;
use crate::circuit_breaker::CircuitBreaker;
use crate::feature_toggles::FeatureToggles;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::work_threads::WorkThreadError;

pub(crate) const LEASE_ROLE: &str = "memory_keeper_cards";
pub(crate) const TICK: Duration = Duration::from_secs(10);
const SWEEP_EVERY: Duration = Duration::from_secs(60);
const BREAKER_THRESHOLD: u32 = 3;
const BREAKER_COOLDOWN_BASE: Duration = Duration::from_secs(30);
const BREAKER_COOLDOWN_MAX: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TickOutcome {
    Built(PassReport),
    Failed,
    NotLeader,
    CircuitOpen,
}

pub(crate) struct Keeper {
    generation: Option<i64>,
    next_sweep: Instant,
    breaker: CircuitBreaker,
}

impl Keeper {
    pub(crate) fn new(now: Instant) -> Self {
        Self {
            generation: None,
            next_sweep: now,
            breaker: CircuitBreaker::new(
                BREAKER_THRESHOLD,
                BREAKER_COOLDOWN_BASE,
                BREAKER_COOLDOWN_MAX,
            ),
        }
    }

    pub(crate) async fn tick(&mut self, persist: &PersistHandle, now: Instant) -> TickOutcome {
        if !persist.holds_worker_role(LEASE_ROLE, TICK).await {
            self.generation = None;
            health::record(LastPass::NotLeader { at: Utc::now() });
            return TickOutcome::NotLeader;
        }
        if self.breaker.should_skip(now) {
            return TickOutcome::CircuitOpen;
        }
        let sweep = self.next_sweep <= now;
        let result = self.pass(persist, sweep).await;
        self.breaker.record(result.is_ok(), now);
        match result {
            Ok(report) => {
                if sweep {
                    self.next_sweep = now + SWEEP_EVERY;
                }
                log_pass(&report, sweep);
                TickOutcome::Built(report)
            }
            Err(error) => {
                tracing::warn!(error = %error, "memory_keeper: card pass failed; cards keep their previous text");
                health::record(LastPass::Failed {
                    at: Utc::now(),
                    error: error.to_string(),
                });
                TickOutcome::Failed
            }
        }
    }

    async fn pass(
        &mut self,
        persist: &PersistHandle,
        sweep: bool,
    ) -> Result<PassReport, WorkThreadError> {
        let generation = persist.thread_card_sources_generation().await?;
        let mut dirty = Vec::new();
        if self.generation != Some(generation) {
            dirty.extend(persist.thread_cards_behind_their_threads().await?);
        }
        if sweep {
            dirty.extend(persist.thread_cards_behind_their_busy_sources().await?);
        }
        let report = rebuild(persist, &merge(dirty)).await?;
        self.generation = Some(generation);
        Ok(report)
    }
}

fn log_pass(report: &PassReport, sweep: bool) {
    health::record(LastPass::Built {
        at: Utc::now(),
        rebuilt: report.rebuilt,
        written: report.written,
    });
    if report.rebuilt > 0 {
        tracing::debug!(
            rebuilt = report.rebuilt,
            written = report.written,
            sweep,
            "memory_keeper: cards rebuilt"
        );
    }
}

fn log_not_spawned(reason: &'static str) {
    tracing::debug!(reason, "memory_keeper: card worker not spawned");
}

fn log_spawned() {
    tracing::info!(
        tick_secs = TICK.as_secs(),
        "memory_keeper: card worker spawned"
    );
}

fn log_toggled_off() {
    tracing::info!(
        "memory_keeper: toggle turned off, card worker exiting; turning it back on needs a restart"
    );
}

fn spawnable(state: &AppState) -> Option<PersistHandle> {
    if !state.feature_toggle_overlay.load().memory_keeper {
        log_not_spawned("toggle off");
        return None;
    }
    let persist = state.persist().cloned();
    if persist.is_none() {
        log_not_spawned("no persistence");
    }
    persist
}

pub(crate) fn spawn_memory_keeper(state: &AppState) {
    let Some(persist) = spawnable(state) else {
        return;
    };
    let toggles = state.feature_toggle_overlay.clone();
    let cache = state.keeper_snapshot.clone();
    state
        .tracker
        .spawn(run(persist, toggles, cache, state.shutdown()));
    log_spawned();
}

async fn run(
    persist: PersistHandle,
    toggles: Arc<ArcSwap<FeatureToggles>>,
    cache: KeeperCache,
    shutdown: CancellationToken,
) {
    health::set_running(true);
    let mut keeper = Keeper::new(Instant::now());
    loop {
        if !toggles.load().memory_keeper {
            log_toggled_off();
            break;
        }
        keeper.tick(&persist, Instant::now()).await;
        refresh_snapshot(&persist, &cache).await;
        tokio::select! {
            biased;
            () = shutdown.cancelled() => break,
            () = tokio::time::sleep(TICK) => {}
        }
    }
    health::set_running(false);
}

async fn refresh_snapshot(persist: &PersistHandle, cache: &KeeperCache) {
    if let Err(error) = refresh(persist, cache).await {
        tracing::warn!(error = %error, "memory_keeper: card snapshot not refreshed; the previous one stays");
    }
}

#[cfg(test)]
#[path = "worker_tests.rs"]
mod worker_tests;
