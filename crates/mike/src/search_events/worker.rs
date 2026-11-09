use std::time::Duration;

use tokio::time::{MissedTickBehavior, interval};

use crate::config::AppState;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::search_events::{USED_BATCH, UsedPass};

const LEASE_ROLE: &str = "search_events_used";
const PASS_INTERVAL: Duration = Duration::from_secs(120);

pub(crate) fn spawn_search_events_worker(state: &AppState) {
    let Some(persist) = state.persist().cloned() else {
        tracing::debug!("search_events: no persistence — not spawning");
        return;
    };
    let shutdown = state.shutdown();
    state.tracker.spawn(async move {
        let mut ticker = interval(PASS_INTERVAL);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => break,
                _ = ticker.tick() => run_pass(&persist).await,
            }
        }
    });
    tracing::info!(
        interval_secs = PASS_INTERVAL.as_secs(),
        "search_events: used-mark worker spawned"
    );
}

pub(super) async fn run_pass(persist: &PersistHandle) {
    if !persist.holds_worker_role(LEASE_ROLE, PASS_INTERVAL).await {
        return;
    }
    match persist.mark_used_search_references(USED_BATCH).await {
        Ok(pass) => report(&pass),
        Err(error) => tracing::warn!(error = %error, "search_events: used-mark pass failed"),
    }
}

fn report(pass: &UsedPass) {
    if pass.with_used == 0 && pass.finalised == 0 {
        return;
    }
    tracing::info!(
        checked = pass.updated,
        with_used = pass.with_used,
        finalised = pass.finalised,
        "search_events: used marks updated"
    );
}
