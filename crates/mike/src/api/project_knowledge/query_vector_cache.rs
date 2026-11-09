use std::collections::{HashMap, VecDeque};
use std::sync::{LazyLock, Mutex, PoisonError};

const CAPACITY: usize = 256;

type Key = (String, String);

#[derive(Default)]
struct Entries {
    vectors: HashMap<Key, Vec<f32>>,
    recency: VecDeque<Key>,
}

impl Entries {
    fn touch(&mut self, key: &Key) {
        if let Some(position) = self.recency.iter().position(|seen| seen == key) {
            self.recency.remove(position);
        }
        self.recency.push_back(key.clone());
    }
}

pub(crate) struct QueryVectorCache {
    capacity: usize,
    entries: Mutex<Entries>,
}

impl QueryVectorCache {
    pub(crate) fn with_capacity(capacity: usize) -> Self {
        Self {
            capacity: capacity.max(1),
            entries: Mutex::new(Entries::default()),
        }
    }

    pub(crate) fn get(&self, model_key: &str, query: &str) -> Option<Vec<f32>> {
        let key = (model_key.to_owned(), query.to_owned());
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let vector = entries.vectors.get(&key).cloned()?;
        entries.touch(&key);
        Some(vector)
    }

    pub(crate) fn put(&self, model_key: &str, query: &str, vector: Vec<f32>) {
        let key = (model_key.to_owned(), query.to_owned());
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        if !entries.vectors.contains_key(&key)
            && entries.vectors.len() >= self.capacity
            && let Some(oldest) = entries.recency.pop_front()
        {
            entries.vectors.remove(&oldest);
        }
        entries.touch(&key);
        entries.vectors.insert(key, vector);
    }

    #[cfg(test)]
    pub(crate) fn len(&self) -> usize {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .vectors
            .len()
    }
}

pub(crate) static QUERY_VECTORS: LazyLock<QueryVectorCache> =
    LazyLock::new(|| QueryVectorCache::with_capacity(CAPACITY));
