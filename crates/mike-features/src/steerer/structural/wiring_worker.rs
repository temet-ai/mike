use std::time::Duration;

use chrono::Utc;
use tokio::time::{MissedTickBehavior, interval};

use super::wiring::{PassOutcome, mark_pass, run_checks};
use super::wiring_store::{Tally, apply};
use tokio_util::sync::CancellationToken;
use tokio_util::task::TaskTracker;

use crate::store::persistence::PersistHandle;

const FIRST_PASS_DELAY: Duration = Duration::from_secs(300);
const PASS_INTERVAL: Duration = Duration::from_secs(6 * 60 * 60);

pub fn spawn_wiring_liveness(
    persist: Option<PersistHandle>,
    tracker: &TaskTracker,
    shutdown: CancellationToken,
) {
    let Some(persist) = persist else {
        tracing::debug!("wiring_liveness: no persistence, not spawning");
        return;
    };
    tracker.spawn(async move {
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
    let stored = apply(persist.pool(), &outcome).await;
    report_stored(&outcome, stored);
    mark_pass(now);
}

fn report_stored(outcome: &PassOutcome, stored: Result<Tally, sqlx::Error>) {
    match stored {
        Ok(tally) => log_finished(outcome.findings.len(), tally),
        Err(error) => log_unstored(&error),
    }
}

fn log_finished(findings: usize, tally: Tally) {
    tracing::info!(
        findings,
        upserted = tally.upserted,
        resolved = tally.resolved,
        "wiring_liveness: pass finished"
    );
}

fn log_unstored(error: &sqlx::Error) {
    tracing::warn!(%error, "wiring_liveness: could not store the findings");
}

fn report_failures(outcome: &PassOutcome) {
    for (kind, error) in &outcome.failed {
        tracing::warn!(check = kind, %error, "wiring_liveness: a check failed");
    }
}
