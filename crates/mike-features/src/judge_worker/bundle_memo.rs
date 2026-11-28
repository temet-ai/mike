use std::future::Future;
use std::sync::Arc;

use dashmap::DashMap;

#[derive(Debug)]
pub struct BundleMemo<T> {
    entries: Arc<DashMap<i64, T>>,
}

impl<T> Clone for BundleMemo<T> {
    fn clone(&self) -> Self {
        Self {
            entries: self.entries.clone(),
        }
    }
}

impl<T> Default for BundleMemo<T> {
    fn default() -> Self {
        Self {
            entries: Arc::new(DashMap::new()),
        }
    }
}

impl<T: Clone> BundleMemo<T> {
    pub async fn get_or_load<F, Fut>(&self, bundle_id: i64, load: F) -> Option<T>
    where
        F: FnOnce() -> Fut,
        Fut: Future<Output = Option<T>>,
    {
        if let Some(hit) = self.entries.get(&bundle_id) {
            return Some(hit.value().clone());
        }
        let loaded = load().await?;
        self.entries.insert(bundle_id, loaded.clone());
        Some(loaded)
    }
}

#[cfg(test)]
#[path = "bundle_memo_tests.rs"]
mod bundle_memo_tests;
