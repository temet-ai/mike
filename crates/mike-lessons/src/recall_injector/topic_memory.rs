use std::collections::HashMap;
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};

pub(super) const MAPPING_LIFETIME: Duration = Duration::from_secs(24 * 60 * 60);
pub(super) const UNMAPPED_LIFETIME: Duration = Duration::from_secs(60 * 60);
pub(super) const CAPACITY: usize = 2_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Known {
    Mapped(String),
    Unmapped,
}

impl Known {
    fn lifetime(&self) -> Duration {
        match self {
            Self::Mapped(_) => MAPPING_LIFETIME,
            Self::Unmapped => UNMAPPED_LIFETIME,
        }
    }
}

#[derive(Debug)]
struct Entry {
    known: Known,
    at: Instant,
}

impl Entry {
    fn live(&self, now: Instant) -> bool {
        now.saturating_duration_since(self.at) < self.known.lifetime()
    }
}

type Key = (String, String);

#[derive(Debug, Default)]
pub(super) struct TopicMemory {
    entries: Mutex<HashMap<Key, Entry>>,
}

impl TopicMemory {
    pub(super) fn get(&self, project: &str, typed: &str, now: Instant) -> Option<Known> {
        let entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        entries
            .get(&(project.to_owned(), typed.to_owned()))
            .filter(|entry| entry.live(now))
            .map(|entry| entry.known.clone())
    }

    pub(super) fn put(&self, project: &str, typed: &str, known: Known, now: Instant) {
        let mut entries = self.entries.lock().unwrap_or_else(PoisonError::into_inner);
        let key = (project.to_owned(), typed.to_owned());
        if !entries.contains_key(&key) && entries.len() >= CAPACITY {
            entries.retain(|_, entry| entry.live(now));
        }
        if !entries.contains_key(&key) && entries.len() >= CAPACITY {
            let oldest = entries
                .iter()
                .min_by_key(|(_, entry)| entry.at)
                .map(|(key, _)| key.clone());
            if let Some(oldest) = oldest {
                entries.remove(&oldest);
            }
        }
        entries.insert(key, Entry { known, at: now });
    }

    #[cfg(test)]
    pub(super) fn len(&self) -> usize {
        self.entries
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .len()
    }

    pub(super) fn expand(&self, project: &str, typed: &[String], now: Instant) -> Vec<String> {
        let mut read = typed.to_vec();
        for topic in typed {
            if let Some(Known::Mapped(mapped)) = self.get(project, topic, now)
                && !read.contains(&mapped)
            {
                read.push(mapped);
            }
        }
        read
    }

    pub(super) fn unasked(&self, project: &str, typed: &[String], now: Instant) -> Vec<String> {
        typed
            .iter()
            .filter(|topic| self.get(project, topic, now).is_none())
            .cloned()
            .collect()
    }
}

static MEMORY: LazyLock<TopicMemory> = LazyLock::new(TopicMemory::default);

pub(super) fn expand(project: &str, typed: &[String]) -> Vec<String> {
    MEMORY.expand(project, typed, Instant::now())
}

pub(super) fn unasked(project: &str, typed: &[String]) -> Vec<String> {
    MEMORY.unasked(project, typed, Instant::now())
}

pub(super) fn mark_asked(project: &str, typed: &[String]) {
    let now = Instant::now();
    for topic in typed {
        MEMORY.put(project, topic, Known::Unmapped, now);
    }
}

pub(super) fn remember(project: &str, typed: &str, mapped: &str) {
    MEMORY.put(
        project,
        typed,
        Known::Mapped(mapped.to_owned()),
        Instant::now(),
    );
}

#[cfg(test)]
#[path = "topic_memory_tests.rs"]
mod tests;
