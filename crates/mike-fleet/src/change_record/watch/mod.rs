pub mod author;
pub mod author_jev;
pub mod author_named;
pub mod checkout;
pub mod git;
pub mod notifications;
pub mod record;
pub mod refs;
pub mod supervisor;
pub mod tree;

#[cfg(test)]
mod tests;

use std::sync::atomic::{AtomicBool, AtomicU64};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use dashmap::DashMap;
use tokio::sync::broadcast;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

pub use git::{Git, GitError};
pub use record::Checkout;
pub use supervisor::spawn_change_record_watcher;
pub use tree::{DeltaKind, FileDelta};

use crate::store::persistence::PersistHandle;

const FEED_CAPACITY: usize = 256;

#[derive(Debug, Clone, Copy)]
pub struct WatchConfig {
    pub notifications: bool,
    pub debounce: Duration,
    pub backstop: Duration,
    pub fallback_poll: Duration,
    pub author_delay: Duration,
    pub edit_window: Duration,
    pub hold_retries: bool,
}

impl Default for WatchConfig {
    fn default() -> Self {
        Self {
            notifications: true,
            debounce: Duration::from_millis(300),
            backstop: Duration::from_secs(30),
            fallback_poll: Duration::from_secs(2),
            author_delay: Duration::from_secs(5),
            edit_window: Duration::from_secs(30),
            hold_retries: false,
        }
    }
}

#[derive(Debug, Default)]
pub struct WatchStats {
    pub ready: AtomicBool,
    pub notifications: AtomicBool,
    pub backstop_polls: AtomicU64,
    pub rescans: AtomicU64,
    pub events_recorded: AtomicU64,
    pub authors_named: AtomicU64,
    pub retries_held: AtomicBool,
}

pub struct Running {
    pub stats: Arc<WatchStats>,
    pub token: CancellationToken,
    pub handle: JoinHandle<()>,
}

struct Feed {
    deltas: broadcast::Sender<Vec<FileDelta>>,
    generation: u64,
}

fn next_generation() -> u64 {
    static GENERATION: AtomicU64 = AtomicU64::new(0);
    GENERATION.fetch_add(1, std::sync::atomic::Ordering::AcqRel)
}

fn feeds() -> &'static DashMap<String, Feed> {
    static FEEDS: OnceLock<DashMap<String, Feed>> = OnceLock::new();
    FEEDS.get_or_init(DashMap::new)
}

pub fn key(root: &str) -> String {
    root.trim_end_matches('/').to_string()
}

pub fn subscribe(root: &str) -> Option<broadcast::Receiver<Vec<FileDelta>>> {
    feeds().get(&key(root)).map(|feed| feed.deltas.subscribe())
}

pub fn is_watching(root: &str) -> bool {
    feeds().contains_key(&key(root))
}

pub fn start(
    persist: PersistHandle,
    checkout: Checkout,
    config: WatchConfig,
    token: CancellationToken,
) -> Running {
    let stats = Arc::new(WatchStats::default());
    stats
        .retries_held
        .store(config.hold_retries, std::sync::atomic::Ordering::Release);
    let root = key(&checkout.root);
    let (deltas, _) = broadcast::channel(FEED_CAPACITY);
    let generation = next_generation();
    feeds().insert(
        root.clone(),
        Feed {
            deltas: deltas.clone(),
            generation,
        },
    );
    let task_stats = stats.clone();
    let task_token = token.clone();
    let checkout = Checkout {
        root: root.clone(),
        ..checkout
    };
    let handle = tokio::spawn(async move {
        match checkout::Watch::open(persist, checkout, config, task_stats, deltas).await {
            Ok(watch) => watch.run(task_token).await,
            Err(err) => {
                tracing::warn!(%root, error = %err, "change record: the checkout could not be read, so it is not watched");
            }
        }
        feeds().remove_if(&root, |_, feed| feed.generation == generation);
    });
    Running {
        stats,
        token,
        handle,
    }
}
