use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use tokio_util::sync::CancellationToken;

use super::{Checkout, Git, Running, WatchConfig, key, start};
use crate::change_record::WorkingCheckout;
use crate::config::AppState;
use crate::feature_toggles::FeatureToggles;
use crate::store::persistence::PersistHandle;

const TICK: Duration = Duration::from_secs(30);
const MAX_BACKOFF: Duration = Duration::from_secs(600);
const ROOT_CACHE: usize = 1024;

fn worked_within() -> chrono::Duration {
    chrono::Duration::minutes(30)
}

pub fn spawn_change_record_watcher(state: &AppState) {
    let Some(persist) = state.persist().cloned() else {
        tracing::info!("change record: no persistence, so no checkout is watched");
        return;
    };
    let toggles = state.feature_toggle_overlay.clone();
    let shutdown = state.shutdown();
    state.tracker.spawn(async move {
        Supervisor::new(persist, WatchConfig::default())
            .supervise(toggles, shutdown)
            .await;
    });
}

pub struct Supervisor {
    persist: PersistHandle,
    config: WatchConfig,
    running: BTreeMap<String, Running>,
    roots: HashMap<String, Option<String>>,
    failures: u32,
}

impl Supervisor {
    pub fn new(persist: PersistHandle, config: WatchConfig) -> Self {
        Self {
            persist,
            config,
            running: BTreeMap::new(),
            roots: HashMap::new(),
            failures: 0,
        }
    }

    pub async fn supervise(
        mut self,
        toggles: Arc<ArcSwap<FeatureToggles>>,
        shutdown: CancellationToken,
    ) {
        let mut delay = TICK;
        loop {
            tokio::select! {
                () = shutdown.cancelled() => break,
                () = tokio::time::sleep(delay) => {}
            }
            delay = if toggles.load().change_record {
                self.tick(&shutdown).await
            } else {
                self.stop_all();
                TICK
            };
        }
        self.stop_all();
    }

    pub async fn tick(&mut self, shutdown: &CancellationToken) -> Duration {
        match self
            .persist
            .checkouts_with_working_sessions(worked_within())
            .await
        {
            Ok(rows) => {
                self.failures = 0;
                let wanted = self.resolve(&rows).await;
                self.apply(&wanted, shutdown);
                TICK
            }
            Err(err) => {
                self.failures = self.failures.saturating_add(1);
                tracing::warn!(error = %err, failures = self.failures, "change record: working checkouts could not be read");
                backoff(self.failures)
            }
        }
    }

    pub fn watching(&self) -> BTreeSet<String> {
        self.running.keys().cloned().collect()
    }

    async fn resolve(&mut self, rows: &[WorkingCheckout]) -> BTreeMap<String, i64> {
        if self.roots.len() > ROOT_CACHE {
            self.roots.clear();
        }
        let mut wanted = BTreeMap::new();
        for row in rows {
            if !self.roots.contains_key(&row.checkout) {
                let root = repository_root(&row.checkout).await;
                self.roots.insert(row.checkout.clone(), root);
            }
            if let Some(Some(root)) = self.roots.get(&row.checkout) {
                wanted.entry(root.clone()).or_insert(row.project_id);
            }
        }
        wanted
    }

    fn apply(&mut self, wanted: &BTreeMap<String, i64>, shutdown: &CancellationToken) {
        self.running.retain(|_, run| !run.handle.is_finished());
        let running = self.watching();
        let (begin, end) = plan(&running, wanted);
        for root in end {
            if let Some(run) = self.running.remove(&root) {
                run.token.cancel();
            }
        }
        for (root, project_id) in begin {
            let checkout = Checkout {
                root: root.clone(),
                project_id,
            };
            let run = start(
                self.persist.clone(),
                checkout,
                self.config,
                shutdown.child_token(),
            );
            self.running.insert(root, run);
        }
    }

    fn stop_all(&mut self) {
        for (_, run) in std::mem::take(&mut self.running) {
            run.token.cancel();
        }
    }
}

async fn repository_root(folder: &str) -> Option<String> {
    if !Path::new(folder).is_dir() {
        return None;
    }
    Git::new(folder).show_toplevel().await.ok().map(|r| key(&r))
}

pub fn plan(
    running: &BTreeSet<String>,
    wanted: &BTreeMap<String, i64>,
) -> (Vec<(String, i64)>, Vec<String>) {
    let begin = wanted
        .iter()
        .filter(|(root, _)| !running.contains(*root))
        .map(|(root, id)| (root.clone(), *id))
        .collect();
    let end = running
        .iter()
        .filter(|root| !wanted.contains_key(*root))
        .cloned()
        .collect();
    (begin, end)
}

pub fn backoff(failures: u32) -> Duration {
    let factor = 1u32 << failures.min(5);
    (TICK * factor).min(MAX_BACKOFF)
}
