use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use arc_swap::ArcSwap;
use chrono::{DateTime, Utc};
use tokio_util::sync::CancellationToken;

use super::shell_folders::ShellFolders;
use super::working::{WAKES, Working, expiry, worked_within};
use super::{Checkout, Git, Running, WatchConfig, key, start};
use crate::base_state::BaseState;
use crate::change_feed::ChangeFeed;
use crate::change_record::WorkingCheckout;
use crate::feature_toggles::FeatureToggles;
use crate::store::persistence::PersistHandle;

const RETRY: Duration = Duration::from_secs(30);
const TOGGLE_CHECK: Duration = Duration::from_secs(30);
const MAX_BACKOFF: Duration = Duration::from_secs(600);
const ROOT_CACHE: usize = 1024;

pub fn spawn_change_record_watcher(state: &BaseState) {
    let Some(persist) = state.persist().cloned() else {
        tracing::info!("change record: no persistence, so no checkout is watched");
        return;
    };
    let toggles = state.feature_toggle_overlay.clone();
    let feed = state.change_feed.clone();
    let shutdown = state.shutdown();
    state.tracker.spawn(async move {
        Supervisor::new(persist, feed, WatchConfig::default())
            .supervise(toggles, shutdown)
            .await;
    });
}

pub struct Supervisor {
    persist: PersistHandle,
    config: WatchConfig,
    working: Working,
    placed: Vec<WorkingCheckout>,
    shell: ShellFolders,
    running: BTreeMap<String, Running>,
    roots: HashMap<String, Option<String>>,
    failures: u32,
    started: bool,
}

struct Wanted {
    projects: BTreeMap<String, i64>,
    newest: BTreeMap<String, DateTime<Utc>>,
    entered: BTreeMap<String, DateTime<Utc>>,
}

impl Supervisor {
    pub fn new(persist: PersistHandle, feed: ChangeFeed, config: WatchConfig) -> Self {
        Self {
            persist,
            config,
            working: Working::new(feed),
            placed: Vec::new(),
            shell: ShellFolders::default(),
            running: BTreeMap::new(),
            roots: HashMap::new(),
            failures: 0,
            started: false,
        }
    }

    pub async fn supervise(
        mut self,
        toggles: Arc<ArcSwap<FeatureToggles>>,
        shutdown: CancellationToken,
    ) {
        loop {
            let since = self.working.feed().generation();
            let pause = if toggles.load().change_record {
                self.step(&shutdown).await
            } else {
                self.stop_all();
                self.working.forget();
                self.shell.forget();
                None
            };
            let wait = pause.unwrap_or_else(|| self.working.wait(Utc::now(), TOGGLE_CHECK));
            let feed = self.working.feed().clone();
            tokio::select! {
                () = shutdown.cancelled() => break,
                () = tokio::time::sleep(wait), if pause.is_some() => {}
                _ = feed.wait_for_change(&WAKES, since, wait), if pause.is_none() => {}
            }
        }
        self.stop_all();
    }

    async fn step(&mut self, shutdown: &CancellationToken) -> Option<Duration> {
        if self.working.due(Utc::now()) {
            return self.tick(shutdown).await;
        }
        if !self.working.requested() {
            return None;
        }
        let wait = self.shell.wait();
        if !wait.is_zero() {
            return Some(wait);
        }
        self.read_shell(shutdown).await;
        None
    }

    pub async fn read_shell(&mut self, shutdown: &CancellationToken) {
        self.take_in_shell_commands().await;
        let wanted = self.wanted().await;
        self.working.expire(expiry(&wanted.newest));
        self.apply(&wanted, shutdown);
    }

    async fn take_in_shell_commands(&mut self) {
        self.working.requests_read();
        let now = Utc::now();
        let (since, after_id) = self.shell.since(now);
        match self
            .persist
            .fleet()
            .shell_commands_since(since, after_id)
            .await
        {
            Ok(commands) => self.shell.add(&commands, now),
            Err(err) => {
                tracing::warn!(error = %err, "change record: new shell commands could not be read");
            }
        }
    }

    async fn wanted(&mut self) -> Wanted {
        let mut rows = self.placed.clone();
        rows.extend(self.shell.rows());
        self.resolve(&rows).await
    }

    pub async fn tick(&mut self, shutdown: &CancellationToken) -> Option<Duration> {
        let stamp = self.working.before_read();
        match self
            .persist
            .fleet()
            .checkouts_with_working_sessions(worked_within())
            .await
        {
            Ok(rows) => {
                self.failures = 0;
                self.placed = rows;
                self.take_in_shell_commands().await;
                let wanted = self.wanted().await;
                self.working
                    .read(stamp, &self.placed, expiry(&wanted.newest));
                self.apply(&wanted, shutdown);
                None
            }
            Err(err) => {
                self.failures = self.failures.saturating_add(1);
                self.working.forget();
                tracing::warn!(error = %err, failures = self.failures, "change record: working checkouts could not be read");
                Some(backoff(self.failures))
            }
        }
    }

    pub fn read_again_at(&self) -> Option<DateTime<Utc>> {
        self.working.expires()
    }

    pub fn watching(&self) -> BTreeSet<String> {
        self.running.keys().cloned().collect()
    }

    async fn resolve(&mut self, rows: &[WorkingCheckout]) -> Wanted {
        if self.roots.len() > ROOT_CACHE {
            self.roots.clear();
        }
        let mut wanted = Wanted {
            projects: BTreeMap::new(),
            newest: BTreeMap::new(),
            entered: BTreeMap::new(),
        };
        for row in rows {
            let Some(root) = self.root_of(&row.checkout).await else {
                continue;
            };
            wanted
                .projects
                .entry(root.clone())
                .or_insert(row.project_id);
            let entered = self
                .shell
                .entered(&row.checkout)
                .unwrap_or(row.newest_request_at);
            let first = wanted.entered.entry(root.clone()).or_insert(entered);
            *first = (*first).min(entered);
            let newest = wanted.newest.entry(root).or_insert(row.newest_request_at);
            *newest = (*newest).max(row.newest_request_at);
        }
        wanted
    }

    async fn root_of(&mut self, checkout: &str) -> Option<String> {
        if self.config.skip_temporary && is_temporary(checkout) {
            return None;
        }
        if !in_repository(checkout) {
            self.roots.remove(checkout);
            return None;
        }
        let gone = matches!(self.roots.get(checkout), Some(Some(root)) if !still_there(root));
        if gone || !self.roots.contains_key(checkout) {
            let root = repository_root(checkout).await;
            self.roots.insert(checkout.to_owned(), root);
        }
        self.roots.get(checkout).cloned().flatten()
    }

    fn apply(&mut self, wanted: &Wanted, shutdown: &CancellationToken) {
        self.running.retain(|_, run| !run.handle.is_finished());
        let running = self.watching();
        let (begin, end) = plan(&running, &wanted.projects);
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
                WatchConfig {
                    catch_up: self.catch_up(wanted.entered.get(&root)),
                    ..self.config
                },
                shutdown.child_token(),
            );
            self.running.insert(root, run);
        }
        self.started = true;
    }

    fn catch_up(&self, entered: Option<&DateTime<Utc>>) -> Option<DateTime<Utc>> {
        let entered = entered.filter(|_| self.started)?;
        Some(*entered - chrono::Duration::seconds(5))
    }

    fn stop_all(&mut self) {
        for (_, run) in std::mem::take(&mut self.running) {
            run.token.cancel();
        }
    }
}

pub fn is_temporary(folder: &str) -> bool {
    let in_tmpdir = std::env::var_os("TMPDIR").is_some_and(|t| Path::new(folder).starts_with(t));
    in_tmpdir
        || folder.starts_with("/tmp/")
        || folder.contains("/.claude/.cache/tmp/")
        || folder.contains("/scratchpad/")
        || folder.ends_with("/scratchpad")
}

pub async fn repository_root(folder: &str) -> Option<String> {
    if !Path::new(folder).is_dir() {
        return None;
    }
    let git = Git::new(folder);
    let root = key(&git.show_toplevel().await.ok()?);
    if same_folder(&root, folder) || git.tracks_anything().await {
        return Some(root);
    }
    tracing::info!(%folder, %root, "change record: the folder holds nothing the enclosing repository tracks, so that repository is not watched");
    None
}

fn in_repository(folder: &str) -> bool {
    let folder = Path::new(folder);
    folder.is_dir() && folder.ancestors().any(|above| above.join(".git").exists())
}

pub fn still_there(root: &str) -> bool {
    Path::new(root).is_dir()
        && std::fs::canonicalize(root).is_ok_and(|real| real == Path::new(root))
}

fn same_folder(root: &str, folder: &str) -> bool {
    let canonical = |p: &str| std::fs::canonicalize(p).ok();
    key(root) == key(folder) || canonical(root).is_some_and(|r| Some(r) == canonical(folder))
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
    (RETRY * factor).min(MAX_BACKOFF)
}
