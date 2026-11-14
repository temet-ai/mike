mod post;
mod scan;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use mike_code::RuleSet;
use tokio_util::sync::CancellationToken;

use super::watch::{Git, watched_roots};
use crate::base_state::BaseState;
use crate::feature_toggles::FeatureToggles;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet_repo::FleetPersist as _;

pub use post::Poster;
pub use scan::{fill, scan_change};

const TICK: Duration = Duration::from_secs(3);
const BATCH: i64 = 50;

pub fn spawn_code_scanner(state: &BaseState) {
    let Some(persist) = state.persist().cloned() else {
        return;
    };
    let rules = match RuleSet::builtin() {
        Ok(rules) => rules,
        Err(err) => {
            tracing::warn!(error = %err, "code scan: the built-in rules did not load, so no edit is scanned");
            return;
        }
    };
    tracing::info!(
        rules = rules.ids().len(),
        version = rules.version(),
        "code scan: rules loaded"
    );
    let worker = Worker {
        persist,
        rules,
        toggles: state.feature_toggle_overlay.clone(),
        cursors: HashMap::new(),
        poster: Poster::new(state.code_notes.clone()),
    };
    let shutdown = state.shutdown();
    state.tracker.spawn(worker.run(shutdown));
}

struct Worker {
    persist: PersistHandle,
    rules: &'static RuleSet,
    toggles: Arc<ArcSwap<FeatureToggles>>,
    cursors: HashMap<String, i64>,
    poster: Poster,
}

impl Worker {
    async fn run(mut self, shutdown: CancellationToken) {
        loop {
            tokio::select! {
                () = shutdown.cancelled() => return,
                () = tokio::time::sleep(TICK) => {}
            }
            self.tick().await;
        }
    }

    async fn tick(&mut self) {
        let current = self.toggles.load();
        if !(current.change_record && current.code_scan) {
            self.cursors.clear();
            return;
        }
        let roots = watched_roots();
        for root in &roots {
            self.scan_checkout(root).await;
        }
        if current.code_findings {
            self.poster.post(&self.persist, &roots).await;
        }
        self.poster.record_delivered(&self.persist).await;
    }

    async fn cursor(&mut self, root: &str) -> Option<i64> {
        if let Some(cursor) = self.cursors.get(root) {
            return Some(*cursor);
        }
        match self.persist.fleet().code_scan_cursor(root).await {
            Ok(cursor) => Some(cursor),
            Err(err) => {
                tracing::warn!(%root, error = %err, "code scan: the scan position could not be read");
                None
            }
        }
    }

    async fn scan_checkout(&mut self, root: &str) {
        let Some(cursor) = self.cursor(root).await else {
            return;
        };
        let pending = match self
            .persist
            .fleet()
            .code_scan_pending(root, cursor, BATCH)
            .await
        {
            Ok(rows) => rows,
            Err(err) => {
                tracing::warn!(%root, error = %err, "code scan: new changes could not be read");
                return;
            }
        };
        let last = self.scan_all(root, cursor, pending).await;
        self.cursors.insert(root.to_string(), last);
    }

    async fn scan_all(
        &self,
        root: &str,
        cursor: i64,
        pending: Vec<crate::store::persistence::change_record::code_scan::PendingChange>,
    ) -> i64 {
        let git = Git::new(root);
        let mut last = cursor;
        for change in pending {
            let record = scan_change(&git, self.rules, root, &change).await;
            if let Err(err) = self.persist.fleet().record_code_scan(&record).await {
                tracing::warn!(%root, path = %change.path, error = %err, "code scan: the scan could not be recorded");
                break;
            }
            last = change.id;
        }
        last
    }
}
