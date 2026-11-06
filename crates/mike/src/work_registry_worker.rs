use std::time::Duration;

use chrono::Utc;
use tokio_util::sync::CancellationToken;

use crate::change_feed::{ChangeFeed, DirtySessions};
use crate::config::AppState;
use crate::store::models::RowChangedTable;
use crate::store::persistence::PersistHandle;

mod delivery;
mod registry;

#[cfg(test)]
mod delivery_tests;
#[cfg(test)]
mod registry_tests;

const LEASE_ROLE: &str = "work_registry";
const HEARTBEAT: Duration = Duration::from_secs(60);
const PASS_GAP: Duration = Duration::from_secs(30);
const SWEEP_EVERY: Duration = Duration::from_secs(15 * 60);
const WAKE_MODE_TTL: Duration = Duration::from_secs(60);
const WATCHED: [RowChangedTable; 1] = [RowChangedTable::Requests];
const ACTIVE_WINDOW_HOURS: i32 = 24;
const DELIVER_RECENT_HOURS: i64 = 2;
const SETTLED_SECONDS: i64 = 60;
const TAIL_BYTES: u64 = 512;
const MAX_DELIVERY_ATTEMPTS: i32 = 6;
const BACKOFF_BASE_SECONDS: i64 = 60;
const BACKOFF_CAP_SECONDS: i64 = 3600;

pub fn spawn_work_registry_worker(state: &AppState) {
    let Some(persist) = state.persist().cloned() else {
        tracing::debug!("work_registry: no persistence — not spawning");
        return;
    };
    let feed = state.change_feed.clone();
    let shutdown = state.shutdown();
    state.tracker.spawn(run(persist, feed, shutdown));
    tracing::info!(
        heartbeat_secs = HEARTBEAT.as_secs(),
        "work_registry: reconciler spawned"
    );
}

async fn run(persist: PersistHandle, feed: ChangeFeed, shutdown: CancellationToken) {
    let dirty = feed.subscribe_sessions(&WATCHED);
    let mut registry = registry::Registry::default();
    loop {
        let since = feed.generation();
        step(&persist, &dirty, &mut registry).await;
        tokio::select! {
            biased;
            () = shutdown.cancelled() => break,
            () = tokio::time::sleep(PASS_GAP) => {}
        }
        let wait = registry.sleep_for(Utc::now(), HEARTBEAT);
        tokio::select! {
            biased;
            () = shutdown.cancelled() => break,
            _ = feed.wait_for_change(&WATCHED, since, wait) => {}
        }
    }
}

async fn step(persist: &PersistHandle, dirty: &DirtySessions, registry: &mut registry::Registry) {
    let drained = dirty.drain();
    if !persist.holds_worker_role(LEASE_ROLE, HEARTBEAT).await {
        registry.release();
        return;
    }
    if let Err(e) = registry.step(persist, drained, Utc::now()).await {
        tracing::warn!(error = %e, "work_registry: pass failed");
        registry.release();
    }
}
