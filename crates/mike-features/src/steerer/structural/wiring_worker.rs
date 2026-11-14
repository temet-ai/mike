use std::time::Duration;

use chrono::Utc;
use tokio::time::{MissedTickBehavior, interval};

use super::wiring::{PassOutcome, mark_pass, run_checks};
use super::wiring_store::apply;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;

const FIRST_PASS_DELAY: Duration = Duration::from_secs(300);
const PASS_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

pub fn spawn_wiring_liveness(state: &AppState) {
    let Some(persist) = state.persist().cloned() else {
        tracing::debug!("wiring_liveness: no persistence, not spawning");
        return;
    };
    let shutdown = state.shutdown();
    state.tracker.spawn(async move {
        tokio::select! {
            biased;
            () = shutdown.cancelled() => return,
            () = tokio::time::sleep(FIRST_PASS_DELAY) => {}
        }
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
        "wiring_liveness: worker spawned"
    );
}

pub async fn run_pass(persist: &PersistHandle) {
    let now = Utc::now();
    let outcome = run_checks(persist.pool(), now).await;
    report_failures(&outcome);
    match apply(persist.pool(), &outcome).await {
        Ok(tally) => tracing::info!(
            findings = outcome.findings.len(),
            upserted = tally.upserted,
            resolved = tally.resolved,
            "wiring_liveness: pass finished"
        ),
        Err(error) => tracing::warn!(%error, "wiring_liveness: could not store the findings"),
    }
    mark_pass(now);
}

fn report_failures(outcome: &PassOutcome) {
    for (kind, error) in &outcome.failed {
        tracing::warn!(check = kind, %error, "wiring_liveness: a check failed");
    }
}
