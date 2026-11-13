use std::future::Future;
use std::hash::Hash;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use serde_json::Value;

use super::{ModelKeys, ProjectIdentity};
use crate::store::persistence::{PersistError, SENTINEL_PROJECT_PATH};

pub const COVERAGE_LIFETIME: Duration = Duration::from_secs(60);
pub const NEAREST_FIRST_LIFETIME: Duration = Duration::from_secs(10 * 60);
pub const NO_PROJECT_LIFETIME: Duration = Duration::from_secs(10 * 60);

#[derive(Debug)]
pub struct Remembered<K: Eq + Hash, V> {
    entries: DashMap<K, (V, u64)>,
    epoch: Instant,
    skew_ms: AtomicU64,
    lifetime: Duration,
    fetched: AtomicUsize,
}

impl<K: Eq + Hash, V: Clone> Remembered<K, V> {
    pub fn new(lifetime: Duration) -> Self {
        Self {
            entries: DashMap::new(),
            epoch: Instant::now(),
            skew_ms: AtomicU64::new(0),
            lifetime,
            fetched: AtomicUsize::new(0),
        }
    }

    fn now_ms(&self) -> u64 {
        let elapsed = u64::try_from(self.epoch.elapsed().as_millis()).unwrap_or(u64::MAX);
        elapsed.saturating_add(self.skew_ms.load(Ordering::Relaxed))
    }

    pub fn get(&self, key: &K) -> Option<V> {
        let (value, stored) = self.entries.get(key).map(|entry| entry.value().clone())?;
        let age = self.now_ms().saturating_sub(stored);
        (u128::from(age) < self.lifetime.as_millis()).then_some(value)
    }

    pub fn put(&self, key: K, value: V) {
        self.entries.insert(key, (value, self.now_ms()));
    }

    pub async fn get_or_fetch<E, F, Fut>(&self, key: K, fetch: F) -> Result<V, E>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Result<V, E>>,
    {
        if let Some(value) = self.get(&key) {
            return Ok(value);
        }
        self.fetched.fetch_add(1, Ordering::Relaxed);
        let value = fetch().await?;
        self.put(key, value.clone());
        Ok(value)
    }

    pub fn clear(&self) {
        self.entries.clear();
    }

    #[cfg(test)]
    pub fn fetched(&self) -> usize {
        self.fetched.load(Ordering::Relaxed)
    }

    pub fn note_fetch(&self) {
        self.fetched.fetch_add(1, Ordering::Relaxed);
    }

    #[cfg(test)]
    pub fn advance(&self, by: Duration) {
        let by = u64::try_from(by.as_millis()).unwrap_or(u64::MAX);
        self.skew_ms.fetch_add(by, Ordering::Relaxed);
    }
}

pub type ModelKey = (i64, String);

#[derive(Debug)]
pub struct KnowledgeMemos {
    pub nearest_first: Remembered<ModelKey, bool>,
    pub coverage: Remembered<ModelKey, Value>,
    pub no_project: Remembered<(), ProjectIdentity>,
}

impl Default for KnowledgeMemos {
    fn default() -> Self {
        Self {
            nearest_first: Remembered::new(NEAREST_FIRST_LIFETIME),
            coverage: Remembered::new(COVERAGE_LIFETIME),
            no_project: Remembered::new(NO_PROJECT_LIFETIME),
        }
    }
}

impl crate::store::persistence::knowledge_repo::KnowledgeDb<'_> {
    pub async fn knowledge_coverage(
        self,
        project_id: i64,
        keys: &ModelKeys,
    ) -> Result<Value, PersistError> {
        self.knowledge_memos()
            .coverage
            .get_or_fetch((project_id, keys.current().to_owned()), || {
                self.knowledge_embedding_health(Some(project_id), keys)
            })
            .await
    }

    pub fn forget_knowledge_coverage(self) {
        self.knowledge_memos().coverage.clear();
    }

    pub async fn no_project_knowledge_project(
        self,
    ) -> Result<Option<ProjectIdentity>, PersistError> {
        let memos = self.knowledge_memos();
        let memo = &memos.no_project;
        if let Some(found) = memo.get(&()) {
            return Ok(Some(found));
        }
        memo.note_fetch();
        let found = crate::metrics::time_db(
            "knowledge_no_project_lookup",
            self.lookup_project(SENTINEL_PROJECT_PATH),
        )
        .await?;
        if let Some(found) = &found {
            memo.put((), found.clone());
        }
        Ok(found)
    }

    pub fn knowledge_memos(self) -> std::sync::Arc<KnowledgeMemos> {
        self.memo::<KnowledgeMemos>()
    }
}

impl KnowledgeMemos {
    #[cfg(test)]
    pub fn counts_fetched(&self) -> (usize, usize, usize) {
        (
            self.nearest_first.fetched(),
            self.coverage.fetched(),
            self.no_project.fetched(),
        )
    }

    #[cfg(test)]
    pub fn advance_all(&self, by: Duration) {
        self.nearest_first.advance(by);
        self.coverage.advance(by);
        self.no_project.advance(by);
    }
}
