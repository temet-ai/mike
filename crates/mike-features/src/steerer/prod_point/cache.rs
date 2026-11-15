use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::LazyLock;
use std::time::{Duration, Instant};

use dashmap::DashMap;

pub const TTL: Duration = Duration::from_secs(24 * 60 * 60);
pub const LIMIT: usize = 10_000;

pub static SHARED: LazyLock<NudgeCache> = LazyLock::new(|| NudgeCache::new(TTL, LIMIT));

pub struct NudgeCache {
    answers: DashMap<u64, (Instant, bool)>,
    ttl: Duration,
    limit: usize,
}

impl NudgeCache {
    pub fn new(ttl: Duration, limit: usize) -> Self {
        Self {
            answers: DashMap::new(),
            ttl,
            limit,
        }
    }

    pub fn key(message: &str, previous_reply_tail: Option<&str>) -> u64 {
        let mut hasher = DefaultHasher::new();
        message.trim().hash(&mut hasher);
        previous_reply_tail.hash(&mut hasher);
        hasher.finish()
    }

    pub fn get(&self, key: u64, now: Instant) -> Option<bool> {
        let entry = self.answers.get(&key).map(|e| *e.value());
        match entry {
            Some((at, answer)) if now.saturating_duration_since(at) < self.ttl => Some(answer),
            Some(_) => {
                self.answers.remove(&key);
                None
            }
            None => None,
        }
    }

    pub fn put(&self, key: u64, answer: bool, now: Instant) {
        if self.answers.len() >= self.limit {
            let ttl = self.ttl;
            self.answers
                .retain(|_, (at, _)| now.saturating_duration_since(*at) < ttl);
            if self.answers.len() >= self.limit {
                self.answers.clear();
            }
        }
        self.answers.insert(key, (now, answer));
    }

    pub fn len(&self) -> usize {
        self.answers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.answers.is_empty()
    }
}
