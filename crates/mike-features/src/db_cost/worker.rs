use crate::store::persistence::state_repo::StatePersist as _;
use std::collections::HashMap;
use std::time::Duration;

use chrono::Utc;
use tokio::time::{MissedTickBehavior, interval};

use super::read::{self, Reading};
use super::report::{self, DbCostReport};
use super::snapshot::{Snapshot, SnapshotRing, Window};
use crate::config::db_cost::DbCostConfig;
use crate::state::AppState;
use crate::store::persistence::PersistHandle;

const MIN_INTERVAL_SECS: u64 = 30;

pub struct Monitor {
    config: DbCostConfig,
    ring: SnapshotRing,
    texts: HashMap<i64, String>,
}

impl Monitor {
    pub fn new(config: DbCostConfig) -> Self {
        let ring = SnapshotRing::new(config.window_secs);
        Self {
            config,
            ring,
            texts: HashMap::new(),
        }
    }

    pub fn config(&self) -> &DbCostConfig {
        &self.config
    }

    pub fn observe(&mut self, snapshot: Snapshot) -> Option<Window> {
        self.ring.push(snapshot);
        self.ring.window()
    }

    pub fn missing_texts(&self, report: &DbCostReport) -> Vec<i64> {
        report::wanted_texts(report)
            .into_iter()
            .filter(|id| !self.texts.contains_key(id))
            .collect()
    }

    pub fn finish(
        &mut self,
        mut report: DbCostReport,
        fetched: HashMap<i64, String>,
    ) -> DbCostReport {
        self.texts.extend(fetched);
        let wanted = report::wanted_texts(&report);
        self.texts.retain(|id, _| wanted.binary_search(id).is_ok());
        report::attach_texts(&mut report, &self.texts);
        report
    }
}

pub async fn tick(persist: &PersistHandle, monitor: &mut Monitor) -> DbCostReport {
    let now = Utc::now();
    let snapshot = match read::read_snapshot(persist).await {
        Ok(Reading::Taken(snapshot)) => snapshot,
        Ok(Reading::Missing) => return report::missing_extension(now, monitor.config()),
        Err(error) => {
            tracing::warn!(%error, "db_cost: reading pg_stat_statements failed");
            return report::read_failed(now, monitor.config(), &error);
        }
    };
    let Some(window) = monitor.observe(snapshot) else {
        return report::warming_up(now, monitor.config());
    };
    let requests = read::count_requests(persist, window.from, window.to)
        .await
        .map_err(|error| tracing::warn!(%error, "db_cost: counting proxied requests failed"))
        .ok();
    let mut built = report::build(&window, requests, monitor.config(), now);
    built.maintenance = persist
        .state()
        .read_database_maintenance()
        .await
        .map_err(|error| tracing::warn!(%error, "db_cost: reading maintenance statistics failed"))
        .ok();
    let missing = monitor.missing_texts(&built);
    let fetched = read::read_texts(persist, &missing)
        .await
        .map_err(|error| tracing::warn!(%error, "db_cost: reading statement texts failed"))
        .unwrap_or_default();
    monitor.finish(built, fetched)
}

pub fn spawn_db_cost_monitor(state: &AppState) {
    let Some(persist) = state.persist().cloned() else {
        tracing::debug!("db_cost: no persistence, not spawning");
        return;
    };
    let config = state.config().db_cost.clone();
    let period = Duration::from_secs(config.interval_secs.max(MIN_INTERVAL_SECS));
    let shutdown = state.shutdown();
    state.tracker.spawn(async move {
        let mut monitor = Monitor::new(config);
        let mut ticker = interval(period);
        ticker.set_missed_tick_behavior(MissedTickBehavior::Skip);
        loop {
            tokio::select! {
                biased;
                () = shutdown.cancelled() => break,
                _ = ticker.tick() => {
                    let report = tick(&persist, &mut monitor).await;
                    super::publish(report);
                }
            }
        }
    });
    tracing::info!(interval_secs = period.as_secs(), "db_cost: monitor spawned");
}

#[cfg(test)]
#[path = "worker_tests.rs"]
pub mod worker_tests;
