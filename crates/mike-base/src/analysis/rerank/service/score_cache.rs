use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};

use dashmap::DashMap;
use sha2::{Digest, Sha256};

use super::super::RerankScore;

pub const CAPACITY: usize = 50_000;
pub const LIFETIME: Duration = Duration::from_secs(30 * 60);
const DROPPED_WHEN_FULL_DIVISOR: usize = 10;

type Digest128 = [u8; 16];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
struct PairKey {
    query: Digest128,
    document: Digest128,
}

pub struct Lookup {
    query: Digest128,
    documents: Vec<Digest128>,
    found: Vec<Option<f32>>,
}

impl Lookup {
    pub fn cached(&self) -> usize {
        self.found.iter().flatten().count()
    }

    pub fn missing(&self) -> Vec<usize> {
        self.found
            .iter()
            .enumerate()
            .filter(|(_, score)| score.is_none())
            .map(|(index, _)| index)
            .collect()
    }

    pub fn merged(&self, fresh: Vec<RerankScore>, missing: &[usize]) -> Vec<RerankScore> {
        let mut scores: Vec<RerankScore> = self
            .found
            .iter()
            .enumerate()
            .filter_map(|(index, score)| score.map(|score| RerankScore { index, score }))
            .collect();
        scores.extend(fresh.into_iter().filter_map(|score| {
            missing.get(score.index).map(|index| RerankScore {
                index: *index,
                score: score.score,
            })
        }));
        scores.sort_by(|a, b| b.score.total_cmp(&a.score).then(a.index.cmp(&b.index)));
        scores
    }
}

pub struct ScoreCache {
    entries: DashMap<PairKey, (f32, u64)>,
    epoch: Instant,
    skew_ms: AtomicU64,
    capacity: usize,
    lifetime: Duration,
}

impl ScoreCache {
    pub fn new() -> Self {
        Self::with_limits(CAPACITY, LIFETIME)
    }

    pub fn with_limits(capacity: usize, lifetime: Duration) -> Self {
        Self {
            entries: DashMap::new(),
            epoch: Instant::now(),
            skew_ms: AtomicU64::new(0),
            capacity: capacity.max(1),
            lifetime,
        }
    }

    fn now_ms(&self) -> u64 {
        let elapsed = u64::try_from(self.epoch.elapsed().as_millis()).unwrap_or(u64::MAX);
        elapsed.saturating_add(self.skew_ms.load(Ordering::Relaxed))
    }

    fn lifetime_ms(&self) -> u64 {
        u64::try_from(self.lifetime.as_millis()).unwrap_or(u64::MAX)
    }

    pub fn lookup(&self, model: &str, query: &str, documents: &[&str]) -> Lookup {
        let query = digest(&[model, query]);
        let now = self.now_ms();
        let keys: Vec<Digest128> = documents.iter().map(|text| digest(&[text])).collect();
        let found = keys
            .iter()
            .map(|document| self.fresh_score(PairKey::new(query, *document), now))
            .collect();
        Lookup {
            query,
            documents: keys,
            found,
        }
    }

    fn fresh_score(&self, key: PairKey, now: u64) -> Option<f32> {
        let (score, stored) = *self.entries.get(&key)?;
        (now.saturating_sub(stored) < self.lifetime_ms()).then_some(score)
    }

    pub fn store(&self, lookup: &Lookup, missing: &[usize], fresh: &[RerankScore]) {
        let now = self.now_ms();
        for score in fresh {
            let Some(document) = missing
                .get(score.index)
                .and_then(|index| lookup.documents.get(*index))
            else {
                continue;
            };
            if self.entries.len() >= self.capacity {
                self.make_room(now);
            }
            self.entries
                .insert(PairKey::new(lookup.query, *document), (score.score, now));
        }
    }

    fn make_room(&self, now: u64) {
        let lifetime = self.lifetime_ms();
        self.entries
            .retain(|_, (_, stored)| now.saturating_sub(*stored) < lifetime);
        if self.entries.len() < self.capacity {
            return;
        }
        let mut stamps: Vec<u64> = self.entries.iter().map(|entry| entry.value().1).collect();
        stamps.sort_unstable();
        let dropped = (self.capacity / DROPPED_WHEN_FULL_DIVISOR).max(1);
        let Some(cutoff) = stamps.get(dropped - 1).copied() else {
            return;
        };
        self.entries.retain(|_, (_, stored)| *stored > cutoff);
    }

    #[cfg(any(test, feature = "test-seams"))]
    pub fn advance(&self, by: Duration) {
        let by = u64::try_from(by.as_millis()).unwrap_or(u64::MAX);
        self.skew_ms.fetch_add(by, Ordering::Relaxed);
    }

    #[cfg(any(test, feature = "test-seams"))]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}

impl PairKey {
    fn new(query: Digest128, document: Digest128) -> Self {
        Self { query, document }
    }
}

fn digest(parts: &[&str]) -> Digest128 {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(u64::try_from(part.len()).unwrap_or(u64::MAX).to_le_bytes());
        hasher.update(part.as_bytes());
    }
    let full = hasher.finalize();
    let mut short = [0u8; 16];
    short.copy_from_slice(&full[..16]);
    short
}

#[cfg(test)]
#[path = "score_cache_tests.rs"]
pub mod score_cache_tests;
