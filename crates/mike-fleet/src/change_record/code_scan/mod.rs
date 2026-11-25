mod post;
pub mod project_rules;
mod scan;

#[cfg(test)]
mod tests;

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use arc_swap::ArcSwap;
use mike_code::RuleSet;
use tokio_util::sync::CancellationToken;

use super::watch::{Git, watched_roots};
use crate::base_state::BaseState;
use crate::feature_toggles::FeatureToggles;
use crate::store::persistence::PersistHandle;
use crate::store::persistence::fleet_repo::FleetPersist as _;

pub use post::Poster;
pub use project_rules::{ActiveRules, ProjectRules, ProjectRulesStatus, project_rules_status};
pub use scan::{fill, scan_change};

use crate::store::persistence::change_record::code_scan::PendingChange;

const TICK: Duration = Duration::from_secs(3);
const BATCH: i64 = 200;
const SETTLE_EVERY: Duration = Duration::from_secs(30);
/// How long a checkout goes without reading new changes when this process
/// wrote none: events another process wrote for it are seen within this.
const PENDING_BACKSTOP: Duration = Duration::from_secs(600);

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
        project: ProjectRules::new(rules),
        toggles: state.feature_toggle_overlay.clone(),
        cursors: HashMap::new(),
        read_at: HashMap::new(),
        settled: None,
        poster: Poster::new(state.code_notes.clone()),
    };
    let shutdown = state.shutdown();
    state.tracker.spawn(worker.run(shutdown));
}

struct Worker {
    persist: PersistHandle,
    project: ProjectRules,
    toggles: Arc<ArcSwap<FeatureToggles>>,
    cursors: HashMap<String, i64>,
    read_at: HashMap<String, Instant>,
    poster: Poster,
    settled: Option<Instant>,
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
            self.read_at.clear();
            return;
        }
        let roots = watched_roots();
        for root in &roots {
            self.scan_checkout(root).await;
        }
        if current.code_findings {
            self.poster.post(&self.persist, &roots).await;
        } else {
            self.poster.mark_stale();
        }
        self.poster.record_delivered(&self.persist).await;
        self.settle().await;
    }

    async fn settle(&mut self) {
        if self.settled.is_some_and(|at| at.elapsed() < SETTLE_EVERY) {
            return;
        }
        self.settled = Some(Instant::now());
        log_settled(self.persist.fleet().settle_code_findings().await);
        self.poster.record_replies(&self.persist).await;
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

    /// Whether new changes can be waiting past `cursor`: one this process wrote,
    /// a first read since the start, or the backstop.
    fn may_have_pending(&self, root: &str, cursor: i64) -> bool {
        let written = self
            .persist
            .fleet()
            .newest_written_change_event(root)
            .is_some_and(|newest| newest > cursor);
        let stale = self
            .read_at
            .get(root)
            .is_none_or(|at| at.elapsed() >= PENDING_BACKSTOP);
        written || stale
    }

    async fn scan_checkout(&mut self, root: &str) {
        let Some(cursor) = self.cursor(root).await else {
            return;
        };
        if !self.may_have_pending(root, cursor) {
            return;
        }
        self.read_at.insert(root.to_string(), Instant::now());
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

    async fn scan_all(&mut self, root: &str, cursor: i64, pending: Vec<PendingChange>) -> i64 {
        let git = Git::new(root);
        let rules = self.project.for_checkout(root).await;
        let mut last = cursor;
        let mut deleted: Vec<String> = Vec::new();
        for change in pending {
            if change.after_sha.is_none() {
                deleted.push(change.path);
                last = change.id;
                continue;
            }
            if !self.supersede(root, &mut deleted).await {
                return cursor;
            }
            if !self.record(&git, rules.clone(), root, &change).await {
                return last;
            }
            self.poster.mark_stale();
            last = change.id;
        }
        if self.supersede(root, &mut deleted).await {
            last
        } else {
            cursor
        }
    }

    async fn record(
        &self,
        git: &Git,
        rules: ActiveRules,
        root: &str,
        change: &PendingChange,
    ) -> bool {
        let change = self.with_moved_from(change).await;
        let record = scan_change(git, rules, root, &change).await;
        match self.persist.fleet().record_code_scan(&record).await {
            Ok(recorded) => {
                if let Some(recorded) = recorded {
                    self.poster.withdraw(&recorded.fixed);
                }
                true
            }
            Err(err) => {
                tracing::warn!(%root, path = %change.path, error = %err, "code scan: the scan could not be recorded");
                false
            }
        }
    }

    async fn with_moved_from(&self, change: &PendingChange) -> PendingChange {
        let mut change = change.clone();
        if change.before_sha.is_none() && change.after_sha.is_some() {
            match self.persist.fleet().code_scan_moved_from(change.id).await {
                Ok(before) => change.before_sha = before,
                Err(err) => {
                    tracing::warn!(path = %change.path, error = %err, "code scan: where a new file came from could not be read");
                }
            }
        }
        change
    }

    async fn supersede(&self, root: &str, deleted: &mut Vec<String>) -> bool {
        if deleted.is_empty() {
            return true;
        }
        deleted.sort();
        deleted.dedup();
        match self
            .persist
            .fleet()
            .supersede_deleted_paths(root, deleted)
            .await
        {
            Ok(superseded) => {
                self.poster.withdraw(&superseded);
                deleted.clear();
                true
            }
            Err(err) => {
                tracing::warn!(%root, paths = deleted.len(), error = %err, "code scan: deleted files could not be recorded");
                false
            }
        }
    }
}

fn log_settled(
    result: Result<
        crate::store::persistence::change_record::code_scan::Settled,
        crate::store::persistence::PersistError,
    >,
) {
    match result {
        Ok(settled) if settled.fixed_by_other + settled.ignored > 0 => log_outcomes(settled),
        Ok(_) => {}
        Err(err) => log_settle_failed(&err),
    }
}

fn log_outcomes(settled: crate::store::persistence::change_record::code_scan::Settled) {
    tracing::info!(
        fixed_by_other = settled.fixed_by_other,
        ignored = settled.ignored,
        "code findings: outcomes settled"
    );
}

fn log_settle_failed(err: &crate::store::persistence::PersistError) {
    tracing::warn!(error = %err, "code findings: outcomes could not be settled");
}
