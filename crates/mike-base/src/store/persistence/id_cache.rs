use std::borrow::Borrow;
use std::hash::Hash;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use dashmap::DashMap;
use uuid::Uuid;

const SESSION_CAPACITY: usize = 50_000;
const PROJECT_CAPACITY: usize = 4_096;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedProject {
    pub id: i64,
    pub path: String,
    pub via_suffix: bool,
}

#[derive(Debug)]
struct Bounded<K: Eq + Hash, V> {
    entries: DashMap<K, (V, u64)>,
    clock: AtomicU64,
    capacity: usize,
}

impl<K: Eq + Hash, V: Clone> Bounded<K, V> {
    fn new(capacity: usize) -> Self {
        Self {
            entries: DashMap::new(),
            clock: AtomicU64::new(0),
            capacity,
        }
    }

    fn get<Q>(&self, key: &Q) -> Option<V>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        self.entries.get(key).map(|entry| entry.value().0.clone())
    }

    fn insert(&self, key: K, value: V) {
        let stamp = self.clock.fetch_add(1, Ordering::Relaxed);
        self.entries.insert(key, (value, stamp));
        if self.entries.len() > self.capacity {
            let keep_after = stamp.saturating_sub((self.capacity / 2) as u64);
            self.entries.retain(|_, (_, at)| *at > keep_after);
        }
    }

    fn clear(&self) {
        self.entries.clear();
    }
}

#[derive(Debug)]
struct Maps {
    sessions: Bounded<Uuid, i64>,
    session_epoch: AtomicU64,
    projects: Bounded<String, CachedProject>,
}

#[derive(Debug, Clone)]
pub struct IdCache {
    maps: Arc<Maps>,
}

impl Default for IdCache {
    fn default() -> Self {
        Self::with_capacity(SESSION_CAPACITY, PROJECT_CAPACITY)
    }
}

impl IdCache {
    fn with_capacity(sessions: usize, projects: usize) -> Self {
        Self {
            maps: Arc::new(Maps {
                sessions: Bounded::new(sessions),
                session_epoch: AtomicU64::new(0),
                projects: Bounded::new(projects),
            }),
        }
    }

    pub fn session(&self, session_uuid: &Uuid) -> Option<i64> {
        self.maps.sessions.get(session_uuid)
    }

    pub fn session_epoch(&self) -> u64 {
        self.maps.session_epoch.load(Ordering::Acquire)
    }

    pub fn remember_session(&self, read_at_epoch: u64, session_uuid: Uuid, id: i64) {
        self.maps.sessions.insert(session_uuid, id);
        if self.session_epoch() != read_at_epoch {
            self.maps.sessions.entries.remove(&session_uuid);
        }
    }

    pub fn forget_sessions(&self) {
        self.maps.session_epoch.fetch_add(1, Ordering::AcqRel);
        self.maps.sessions.clear();
    }

    pub fn project(&self, path: &str) -> Option<CachedProject> {
        self.maps.projects.get(path)
    }

    pub fn remember_project(&self, path: &str, project: CachedProject) {
        self.maps.projects.insert(path.to_owned(), project);
    }

    pub fn forget_projects(&self) {
        self.maps.projects.clear();
    }
}

#[cfg(test)]
#[path = "id_cache_tests.rs"]
pub mod id_cache_tests;
