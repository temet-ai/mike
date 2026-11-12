pub mod backtest;
pub mod calibration_block;
pub mod effective;
mod fire_pass;
pub mod proposer;
pub mod snapshot;
pub mod threshold;
pub mod threshold_pass;
pub mod weighting;

use std::sync::{Arc, LazyLock};
use std::time::Duration;

use arc_swap::ArcSwap;
use chrono::Utc;
use tokio_util::sync::CancellationToken;

use crate::config::AppState;
use crate::eval::bundles::BundleCause;
use crate::feature_toggles::FeatureToggles;
use crate::store::persistence::PersistHandle;

pub use snapshot::CrossSessionSnapshot;

pub const TICK: Duration = Duration::from_secs(60);

static SNAPSHOT: LazyLock<ArcSwap<CrossSessionSnapshot>> =
    LazyLock::new(|| ArcSwap::from_pointee(CrossSessionSnapshot::default()));

pub fn current() -> Arc<CrossSessionSnapshot> {
    SNAPSHOT.load_full()
}

pub fn install(snapshot: CrossSessionSnapshot) {
    SNAPSHOT.store(Arc::new(snapshot));
}

pub struct CrossSessionWorker {
    pub persist: PersistHandle,
    pub toggles: Arc<ArcSwap<FeatureToggles>>,
    pub publisher: Option<Arc<crate::eval::bundles::resolver::BundlePublisher>>,
}

#[derive(Debug, Clone, Default, PartialEq, serde::Serialize)]
pub struct TickReport {
    pub rollup_id: Option<i64>,
    pub new_generation: bool,
    pub threshold_changes: Vec<i64>,
    pub pass_outcome: Option<String>,
    pub proposer: Option<proposer::ProposerReport>,
    pub republished: bool,
}

impl CrossSessionWorker {
    pub async fn tick(
        &self,
        last_rollup: &mut Option<i64>,
    ) -> Result<TickReport, crate::store::persistence::PersistError> {
        let before = current();
        let fresh = CrossSessionSnapshot::load_now(&self.persist).await?;
        let rollup = fresh.rollup.map(|r| r.id);
        let mut report = TickReport {
            rollup_id: rollup,
            new_generation: rollup.is_some() && rollup != *last_rollup,
            ..TickReport::default()
        };
        let snapshot = if report.new_generation {
            let pass =
                threshold_pass::run_threshold_pass(&self.persist, &fresh, Utc::now()).await?;
            report.threshold_changes = pass.changes;
            report.pass_outcome = Some(pass.outcome.to_owned());
            if self.toggles.load().directive_proposer
                && let Some(id) = rollup
            {
                report.proposer =
                    Some(proposer::run_proposer_pass(&self.persist, &fresh.settings, id).await?);
            }
            *last_rollup = rollup;
            CrossSessionSnapshot::load_now(&self.persist).await?
        } else {
            fresh
        };
        let changed = snapshot.describes() != before.describes();
        let guard = snapshot.guard_steerer_id;
        install(snapshot);
        if changed && let (Some(publisher), Some(guard)) = (&self.publisher, guard) {
            publisher
                .republish(BundleCause::steerer(
                    guard,
                    report.threshold_changes.clone(),
                ))
                .await;
            report.republished = true;
        }
        Ok(report)
    }

    pub async fn run(self, shutdown: CancellationToken) {
        let mut last_rollup = None;
        let mut interval = tokio::time::interval(TICK);
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        let mut failures: u32 = 0;
        loop {
            tokio::select! {
                biased;
                _ = shutdown.cancelled() => return,
                _ = interval.tick() => {
                    match self.tick(&mut last_rollup).await {
                        Ok(_) => failures = 0,
                        Err(e) => {
                            failures = failures.saturating_add(1);
                            tracing::warn!(error = %e, failures, "cross_session: tick failed");
                            let backoff = TICK.saturating_mul(failures.min(30));
                            tokio::time::sleep(backoff).await;
                        }
                    }
                }
            }
        }
    }
}

pub fn spawn_cross_session_worker(state: &AppState) {
    let Some(persist) = state.persist.clone() else {
        tracing::info!("cross_session: not spawning, no persistence");
        return;
    };
    let worker = CrossSessionWorker {
        persist,
        toggles: state.feature_toggle_overlay.clone(),
        publisher: state.steerer_service().and_then(|s| s.bundle_publisher()),
    };
    let shutdown = state.shutdown();
    state.tracker.spawn(worker.run(shutdown));
}
