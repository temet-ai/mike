use std::fmt;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, OnceLock};

use dashmap::DashMap;
use uuid::Uuid;

use super::PersistHandle;
use crate::change_feed::{ChangeFeed, Generation};
use crate::store::models::RowChangedTable;

const CAPACITY: usize = 4096;

#[derive(Clone, Copy)]
pub struct Scope {
    pub tables: &'static [RowChangedTable],
    pub per_session: &'static [RowChangedTable],
}

const BRIEFS: Scope = Scope {
    tables: &[],
    per_session: &[
        RowChangedTable::LaunchLaunches,
        RowChangedTable::MailboxMessages,
    ],
};

#[derive(Debug, Clone, Copy)]
pub struct Ticket {
    watermark: Generation,
    revision: u64,
}

pub struct Memo<V> {
    scope: Scope,
    revision: AtomicU64,
    entries: DashMap<Uuid, (Vec<Generation>, V)>,
}

impl<V> Memo<V> {
    pub fn new(scope: Scope) -> Self {
        Self {
            scope,
            revision: AtomicU64::new(0),
            entries: DashMap::new(),
        }
    }

    fn forget_all(&self) {
        self.revision.fetch_add(1, Ordering::AcqRel);
        self.entries.clear();
    }
}

pub struct Bound<'a, V> {
    feed: &'a ChangeFeed,
    memo: &'a Memo<V>,
}

impl<V: Clone> Bound<'_, V> {
    fn stamp(&self, session: Uuid) -> Vec<Generation> {
        self.feed.generations(
            self.memo.scope.tables,
            self.memo.scope.per_session,
            &[session.to_string()],
        )
    }

    pub fn get(&self, session: Uuid) -> Option<V> {
        let entry = self.memo.entries.get(&session)?;
        (entry.0 == self.stamp(session)).then(|| entry.1.clone())
    }

    pub fn ticket(&self) -> Ticket {
        Ticket {
            watermark: self.feed.generation(),
            revision: self.memo.revision.load(Ordering::Acquire),
        }
    }

    pub fn remember(&self, session: Uuid, ticket: Ticket, value: V) {
        let stamp = self.stamp(session);
        let moved = stamp
            .iter()
            .any(|generation| *generation > ticket.watermark);
        if moved || self.memo.revision.load(Ordering::Acquire) != ticket.revision {
            return;
        }
        if self.memo.entries.len() >= CAPACITY && !self.memo.entries.contains_key(&session) {
            self.memo.entries.clear();
        }
        self.memo.entries.insert(session, (stamp, value));
    }
}

struct Inner {
    feed: OnceLock<ChangeFeed>,
    briefs: Memo<()>,
}

#[derive(Clone)]
pub struct FeedMemos {
    inner: Arc<Inner>,
}

impl Default for FeedMemos {
    fn default() -> Self {
        Self {
            inner: Arc::new(Inner {
                feed: OnceLock::new(),
                briefs: Memo::new(BRIEFS),
            }),
        }
    }
}

impl fmt::Debug for FeedMemos {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("FeedMemos")
            .field("attached", &self.inner.feed.get().is_some())
            .field("briefs", &self.inner.briefs.entries.len())
            .finish()
    }
}

impl FeedMemos {
    pub fn bound<'a, V>(&'a self, memo: &'a Memo<V>) -> Option<Bound<'a, V>> {
        self.inner.feed.get().map(|feed| Bound { feed, memo })
    }

    pub fn briefs(&self) -> Option<Bound<'_, ()>> {
        self.bound(&self.inner.briefs)
    }

    pub fn forget_briefs(&self) {
        self.inner.briefs.forget_all();
    }

    pub fn record(&self, table: RowChangedTable, session: Option<Uuid>) {
        if let Some(feed) = self.inner.feed.get() {
            let session = session.map(|s| s.to_string());
            feed.record(table, session.as_deref());
        }
    }
}

impl PersistHandle {
    pub fn attach_change_feed(&self, feed: ChangeFeed) {
        if self.feed_memos.inner.feed.set(feed).is_err() {
            tracing::debug!("the change feed was already attached to this persistence handle");
        }
    }

    pub fn launches_changed(&self, session: Option<Uuid>) {
        self.feed_memos
            .record(RowChangedTable::LaunchLaunches, session);
    }

    pub fn fleets_changed(&self) {
        self.feed_memos.record(RowChangedTable::LaunchFleets, None);
    }
}

#[cfg(test)]
#[path = "feed_memo_tests.rs"]
pub mod feed_memo_tests;
