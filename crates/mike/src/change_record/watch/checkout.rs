use crate::store::persistence::fleet_repo::FleetPersist as _;
use std::collections::{BTreeSet, VecDeque};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use chrono::{DateTime, Utc};
use tokio::sync::{broadcast, mpsc};
use tokio::time::{Instant, MissedTickBehavior};
use tokio_util::sync::CancellationToken;

use super::git::{Git, GitError};
use super::notifications::{Notifier, Places, QUEUE, Signal};
use super::record::{self, Checkout, Source, Unnamed};
use super::refs::RefState;
use super::tree::{FileDelta, KnownTree};
use super::{WatchConfig, WatchStats};
use crate::store::persistence::PersistHandle;

pub(super) struct Watch {
    persist: PersistHandle,
    git: Git,
    checkout: Checkout,
    config: WatchConfig,
    stats: Arc<WatchStats>,
    feed: broadcast::Sender<Vec<FileDelta>>,
    tree: KnownTree,
    refs: RefState,
    notifier: Option<Notifier>,
    rx: mpsc::Receiver<Signal>,
    overflow: Arc<AtomicBool>,
    pending: VecDeque<(Instant, Vec<Unnamed>)>,
}

struct Burst {
    at: DateTime<Utc>,
    paths: BTreeSet<String>,
    refs: bool,
    rescan: bool,
}

impl Watch {
    pub(super) async fn open(
        persist: PersistHandle,
        checkout: Checkout,
        config: WatchConfig,
        stats: Arc<WatchStats>,
        feed: broadcast::Sender<Vec<FileDelta>>,
    ) -> Result<Self, GitError> {
        let git = Git::new(checkout.root.clone());
        let tree = KnownTree::baseline(&git).await?;
        let refs = RefState::baseline(&git).await?;
        let (git_dir, common_dir) = git.git_dirs().await?;
        let places = Places {
            root: checkout.root.clone().into(),
            git_dir,
            common_dir,
        };
        let (tx, rx) = mpsc::channel(QUEUE);
        let overflow = Arc::new(AtomicBool::new(false));
        let notifier = if config.notifications {
            start_notifier(&places, &tree, tx, overflow.clone())
        } else {
            None
        };
        stats
            .notifications
            .store(notifier.is_some(), Ordering::Release);
        Ok(Self {
            persist,
            git,
            checkout,
            config,
            stats,
            feed,
            tree,
            refs,
            notifier,
            rx,
            overflow,
            pending: VecDeque::new(),
        })
    }

    pub(super) async fn run(mut self, token: CancellationToken) {
        let every = if self.notifier.is_some() {
            self.config.backstop
        } else {
            self.config.fallback_poll
        };
        let mut backstop = tokio::time::interval_at(Instant::now() + every, every);
        backstop.set_missed_tick_behavior(MissedTickBehavior::Delay);
        self.stats.ready.store(true, Ordering::Release);
        loop {
            let due = self.pending.front().map(|(due, _)| *due);
            tokio::select! {
                () = token.cancelled() => break,
                signal = self.rx.recv(), if self.notifier.is_some() => match signal {
                    Some(first) => self.on_burst(first).await,
                    None => self.notifier = None,
                },
                _ = backstop.tick() => {
                    self.stats.backstop_polls.fetch_add(1, Ordering::AcqRel);
                    self.full_pass(every).await;
                }
                () = sleep_until(due) => self.name_due().await,
            }
        }
    }

    async fn on_burst(&mut self, first: Signal) {
        let burst = self.collect(first).await;
        let looked_back = self.config.debounce + std::time::Duration::from_secs(2);
        if burst.rescan || self.overflow.swap(false, Ordering::AcqRel) {
            self.stats.rescans.fetch_add(1, Ordering::AcqRel);
            self.full_pass(looked_back).await;
            return;
        }
        if !burst.paths.is_empty() {
            match self.tree.observe_paths(&self.git, &burst.paths).await {
                Ok(observed) => {
                    if let Some(notifier) = self.notifier.as_mut() {
                        notifier.add_dirs(observed.new_dirs);
                    }
                    self.record_files(observed.deltas, burst.at, Source::Notification, looked_back)
                        .await;
                }
                Err(err) => self.warn("changed paths could not be read", &err),
            }
        }
        if burst.refs {
            self.observe_refs(burst.at, looked_back).await;
        }
    }

    async fn collect(&mut self, first: Signal) -> Burst {
        let mut burst = Burst {
            at: Utc::now(),
            paths: BTreeSet::new(),
            refs: false,
            rescan: false,
        };
        let deadline = Instant::now() + self.config.debounce;
        let mut next = Some(first);
        while let Some(signal) = next {
            match signal {
                Signal::Path(path) => {
                    burst.paths.insert(path);
                }
                Signal::Refs => burst.refs = true,
                Signal::Rescan => burst.rescan = true,
            }
            next = tokio::time::timeout_at(deadline, self.rx.recv())
                .await
                .ok()
                .flatten();
        }
        burst
    }

    async fn full_pass(&mut self, looked_back: std::time::Duration) {
        let at = Utc::now();
        match self.tree.backstop(&self.git).await {
            Ok(deltas) => {
                self.record_files(deltas, at, Source::Backstop, looked_back)
                    .await
            }
            Err(err) => self.warn("the backstop status could not be read", &err),
        }
        self.observe_refs(at, looked_back).await;
    }

    async fn observe_refs(&mut self, at: DateTime<Utc>, looked_back: std::time::Duration) {
        let moves = match self.refs.observe(&self.git).await {
            Ok(moves) => moves,
            Err(err) => return self.warn("the branch refs could not be read", &err),
        };
        if moves.is_empty() {
            return;
        }
        let (events, seats) = record::ref_events(&self.git, &self.checkout, &moves, at).await;
        let Some(ids) = self.insert(&events).await else {
            return;
        };
        let back = chrono::Duration::from_std(looked_back).unwrap_or_default();
        let items = record::ref_unnamed(&ids, &events, &seats, at, back);
        self.schedule(items);
    }

    async fn record_files(
        &mut self,
        deltas: Vec<FileDelta>,
        at: DateTime<Utc>,
        source: Source,
        looked_back: std::time::Duration,
    ) {
        if deltas.is_empty() {
            return;
        }
        let events = record::file_events(&self.checkout, &deltas, at, source);
        if let Some(ids) = self.insert(&events).await {
            let back = chrono::Duration::from_std(looked_back).unwrap_or_default();
            let edit = chrono::Duration::from_std(self.config.edit_window).unwrap_or_default();
            let items = record::file_unnamed(&ids, &deltas, at, back, edit);
            self.schedule(items);
        }
        if self.feed.receiver_count() > 0 && self.feed.send(deltas).is_err() {
            tracing::debug!(root = %self.checkout.root, "change record: the fleet feed closed");
        }
    }

    async fn insert(&self, events: &[crate::change_record::NewChangeEvent]) -> Option<Vec<i64>> {
        match self.persist.fleet().insert_change_events(events).await {
            Ok(ids) => {
                self.stats
                    .events_recorded
                    .fetch_add(ids.len() as u64, Ordering::AcqRel);
                Some(ids)
            }
            Err(err) => {
                tracing::warn!(root = %self.checkout.root, error = %err, "change record: events could not be stored");
                None
            }
        }
    }

    fn schedule(&mut self, items: Vec<Unnamed>) {
        if !items.is_empty() {
            self.pending
                .push_back((Instant::now() + self.config.author_delay, items));
            self.pending.make_contiguous().sort_by_key(|(due, _)| *due);
        }
    }

    fn schedule_again(&mut self, items: Vec<Unnamed>) {
        if !items.is_empty() {
            self.pending
                .push_back((Instant::now() + self.config.author_delay * 6, items));
            self.pending.make_contiguous().sort_by_key(|(due, _)| *due);
        }
    }

    async fn name_due(&mut self) {
        let now = Instant::now();
        while self.pending.front().is_some_and(|(due, _)| *due <= now) {
            let Some((_, items)) = self.pending.pop_front() else {
                break;
            };
            if self.stats.retries_held.load(Ordering::Acquire)
                && items.iter().any(Unnamed::is_retry)
            {
                self.schedule(items);
                continue;
            }
            let mut again = Vec::new();
            for item in &items {
                match record::name(&self.persist, &self.git, item).await {
                    Ok(retry) => again.extend(retry),
                    Err(err) => {
                        tracing::warn!(root = %self.checkout.root, error = %err, "change record: an author could not be named");
                    }
                }
            }
            self.schedule_again(again);
            self.stats
                .authors_named
                .fetch_add(items.len() as u64, Ordering::AcqRel);
        }
    }

    fn warn(&self, what: &str, err: &GitError) {
        tracing::warn!(root = %self.checkout.root, error = %err, "change record: {what}");
    }
}

fn start_notifier(
    places: &Places,
    tree: &KnownTree,
    tx: mpsc::Sender<Signal>,
    overflow: Arc<AtomicBool>,
) -> Option<Notifier> {
    match Notifier::start(places, tree.directories(), tx, overflow) {
        Ok(notifier) => Some(notifier),
        Err(err) => {
            tracing::warn!(root = %places.root.display(), error = %err, "change record: file notifications could not start; polling instead");
            None
        }
    }
}

async fn sleep_until(due: Option<Instant>) {
    match due {
        Some(due) => tokio::time::sleep_until(due).await,
        None => std::future::pending().await,
    }
}
