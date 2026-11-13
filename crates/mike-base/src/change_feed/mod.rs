use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};
use std::time::Duration;

use tokio::sync::broadcast::{self, error::RecvError};
use tokio::sync::watch;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use crate::store::models::{ProxyEvent, RowChangedTable};

#[cfg(test)]
pub mod change_feed_tests;

pub type Generation = u64;

const MAX_TRACKED_SESSIONS: usize = 8192;

#[derive(Clone)]
pub struct ChangeFeed {
    inner: Arc<Inner>,
}

struct Inner {
    state: Mutex<FeedState>,
    tick: watch::Sender<Generation>,
    epoch: u64,
    fed: AtomicBool,
}

#[derive(Default)]
struct FeedState {
    current: Generation,
    resynced_at: Generation,
    sessions_forgotten_at: Generation,
    tables: HashMap<RowChangedTable, Generation>,
    sessions: HashMap<(RowChangedTable, String), Generation>,
    subscribers: Vec<Weak<Subscriber>>,
}

struct Subscriber {
    tables: Vec<RowChangedTable>,
    dirt: Mutex<Dirt>,
}

#[derive(Default)]
struct Dirt {
    sessions: HashSet<String>,
    everything: bool,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Drained {
    pub sessions: Vec<String>,
    pub everything: bool,
}

pub struct DirtySessions {
    subscriber: Arc<Subscriber>,
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl FeedState {
    fn bump(&mut self) -> Generation {
        self.current += 1;
        self.current
    }

    fn table_generation(&self, table: RowChangedTable) -> Generation {
        self.tables
            .get(&table)
            .copied()
            .unwrap_or(0)
            .max(self.resynced_at)
    }

    fn session_generation(&self, table: RowChangedTable, session: &str) -> Generation {
        self.sessions
            .get(&(table, session.to_owned()))
            .copied()
            .unwrap_or(self.sessions_forgotten_at)
            .max(self.resynced_at)
    }

    fn remember_session(&mut self, table: RowChangedTable, session: &str, at: Generation) {
        let key = (table, session.to_owned());
        if self.sessions.len() >= MAX_TRACKED_SESSIONS && !self.sessions.contains_key(&key) {
            self.sessions.clear();
            self.sessions_forgotten_at = at;
        }
        self.sessions.insert(key, at);
    }

    fn live_subscribers(&mut self) -> Vec<Arc<Subscriber>> {
        self.subscribers.retain(|weak| weak.strong_count() > 0);
        self.subscribers.iter().filter_map(Weak::upgrade).collect()
    }

    fn mark_dirty(&mut self, table: RowChangedTable, session: &str) {
        for subscriber in self.live_subscribers() {
            if subscriber.tables.contains(&table) {
                lock(&subscriber.dirt).sessions.insert(session.to_owned());
            }
        }
    }

    fn mark_everything(&mut self) {
        for subscriber in self.live_subscribers() {
            lock(&subscriber.dirt).everything = true;
        }
    }
}

impl Default for ChangeFeed {
    fn default() -> Self {
        Self::new()
    }
}

impl ChangeFeed {
    pub fn new() -> Self {
        let (tick, _) = watch::channel(0);
        Self {
            inner: Arc::new(Inner {
                state: Mutex::new(FeedState::default()),
                tick,
                epoch: uuid::Uuid::new_v4().as_u64_pair().0,
                fed: AtomicBool::new(false),
            }),
        }
    }

    pub fn observe(&self, event: &ProxyEvent) {
        match event {
            ProxyEvent::RowChanged {
                table, session_id, ..
            } => self.record(*table, session_id.as_deref()),
            ProxyEvent::ResyncRequired { .. } => self.resync(),
            _ => {}
        }
    }

    pub fn record(&self, table: RowChangedTable, session: Option<&str>) {
        let current = {
            let mut state = lock(&self.inner.state);
            let current = state.bump();
            state.tables.insert(table, current);
            if let Some(session) = session {
                state.remember_session(table, session, current);
                state.mark_dirty(table, session);
            }
            current
        };
        self.inner.tick.send_replace(current);
    }

    pub fn resync(&self) {
        let current = {
            let mut state = lock(&self.inner.state);
            let current = state.bump();
            state.resynced_at = current;
            state.mark_everything();
            current
        };
        self.inner.tick.send_replace(current);
    }

    pub fn is_fed(&self) -> bool {
        self.inner.fed.load(Ordering::Acquire)
    }

    pub fn generation(&self) -> Generation {
        lock(&self.inner.state).current
    }

    #[cfg(test)]
    pub fn table_generation(&self, table: RowChangedTable) -> Generation {
        lock(&self.inner.state).table_generation(table)
    }

    #[cfg(test)]
    pub fn session_generation(&self, table: RowChangedTable, session: &str) -> Generation {
        lock(&self.inner.state).session_generation(table, session)
    }

    pub fn epoch(&self) -> u64 {
        self.inner.epoch
    }

    pub fn generations(
        &self,
        tables: &[RowChangedTable],
        per_session: &[RowChangedTable],
        sessions: &[String],
    ) -> Vec<Generation> {
        let state = lock(&self.inner.state);
        let mut found = Vec::with_capacity(tables.len() + per_session.len() * sessions.len());
        found.extend(tables.iter().map(|table| state.table_generation(*table)));
        for session in sessions {
            found.extend(
                per_session
                    .iter()
                    .map(|table| state.session_generation(*table, session)),
            );
        }
        found
    }

    pub fn changed_since(&self, tables: &[RowChangedTable], since: Generation) -> bool {
        let state = lock(&self.inner.state);
        tables
            .iter()
            .any(|table| state.table_generation(*table) > since)
    }

    pub async fn wait_for_change(
        &self,
        tables: &[RowChangedTable],
        since: Generation,
        timeout: Duration,
    ) -> Generation {
        let deadline = Instant::now() + timeout;
        let mut tick = self.inner.tick.subscribe();
        while !self.changed_since(tables, since) {
            match tokio::time::timeout_at(deadline, tick.changed()).await {
                Ok(Ok(())) => {}
                Ok(Err(_)) | Err(_) => break,
            }
        }
        self.generation()
    }

    pub fn subscribe_sessions(&self, tables: &[RowChangedTable]) -> DirtySessions {
        let subscriber = Arc::new(Subscriber {
            tables: tables.to_vec(),
            dirt: Mutex::new(Dirt {
                sessions: HashSet::new(),
                everything: true,
            }),
        });
        lock(&self.inner.state)
            .subscribers
            .push(Arc::downgrade(&subscriber));
        DirtySessions { subscriber }
    }
}

impl DirtySessions {
    pub fn drain(&self) -> Drained {
        let mut dirt = lock(&self.subscriber.dirt);
        let mut sessions: Vec<String> = dirt.sessions.drain().collect();
        sessions.sort_unstable();
        Drained {
            sessions,
            everything: std::mem::take(&mut dirt.everything),
        }
    }
}

pub async fn pump(
    feed: ChangeFeed,
    mut events: broadcast::Receiver<ProxyEvent>,
    shutdown: CancellationToken,
) {
    loop {
        tokio::select! {
            biased;
            () = shutdown.cancelled() => break,
            received = events.recv() => match received {
                Ok(event) => feed.observe(&event),
                Err(RecvError::Lagged(_)) => feed.resync(),
                Err(RecvError::Closed) => break,
            },
        }
    }
}

pub fn spawn_change_feed(
    feed: &ChangeFeed,
    persist: Option<&crate::store::persistence::PersistHandle>,
    events: tokio::sync::broadcast::Receiver<ProxyEvent>,
    shutdown: CancellationToken,
    tracker: &tokio_util::task::TaskTracker,
) {
    let feed = feed.clone();
    feed.inner.fed.store(true, Ordering::Release);
    if let Some(persist) = persist {
        persist.attach_change_feed(feed.clone());
    }
    tracker.spawn(pump(feed, events, shutdown));
}
