use serde::Serialize;
use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
pub struct RequestId(u64);

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

pub const FALLBACK_BIT: u64 = 1u64 << 63;

static FALLBACK_COUNTER: AtomicU64 = AtomicU64::new(1);

pub fn is_fallback(id: u64) -> bool {
    id & FALLBACK_BIT != 0
}

impl RequestId {
    pub fn next() -> Self {
        Self(NEXT_ID.fetch_add(1, Ordering::Relaxed))
    }

    pub fn fallback() -> Self {
        let n = FALLBACK_COUNTER.fetch_add(1, Ordering::Relaxed);
        Self(FALLBACK_BIT | n)
    }

    pub fn from_raw(val: u64) -> Self {
        Self(val)
    }

    pub fn as_u64(self) -> u64 {
        self.0
    }
}

impl std::fmt::Display for RequestId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}
