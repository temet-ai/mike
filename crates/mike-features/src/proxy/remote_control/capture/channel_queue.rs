use std::collections::HashMap;
use std::future::Future;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

const MAX_BATCH: usize = 64;

pub struct ChannelQueues<T> {
    pending: Arc<Mutex<HashMap<String, Vec<T>>>>,
}

impl<T> Clone for ChannelQueues<T> {
    fn clone(&self) -> Self {
        Self {
            pending: Arc::clone(&self.pending),
        }
    }
}

impl<T> Default for ChannelQueues<T> {
    fn default() -> Self {
        Self {
            pending: Arc::default(),
        }
    }
}

impl<T> ChannelQueues<T> {
    fn lock(&self) -> MutexGuard<'_, HashMap<String, Vec<T>>> {
        self.pending.lock().unwrap_or_else(PoisonError::into_inner)
    }

    pub fn push(&self, channel: &str, item: T) -> bool {
        let mut pending = self.lock();
        if let Some(queue) = pending.get_mut(channel) {
            queue.push(item);
            return false;
        }
        pending.insert(channel.to_owned(), vec![item]);
        true
    }

    fn next_batch(&self, channel: &str) -> Option<Vec<T>> {
        let mut pending = self.lock();
        let queue = pending.get_mut(channel)?;
        if queue.is_empty() {
            pending.remove(channel);
            return None;
        }
        let take = queue.len().min(MAX_BATCH);
        Some(queue.drain(..take).collect())
    }

    pub async fn drain<F, Fut>(self, channel: String, mut write: F)
    where
        F: FnMut(Vec<T>) -> Fut,
        Fut: Future<Output = ()>,
    {
        while let Some(batch) = self.next_batch(&channel) {
            write(batch).await;
        }
    }
}

#[cfg(test)]
#[path = "channel_queue_tests.rs"]
pub mod channel_queue_tests;
