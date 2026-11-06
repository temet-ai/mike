use std::collections::{HashMap, HashSet};
use std::sync::{LazyLock, Mutex, PoisonError};
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::compress::manifest::sha256_hex;

const DEFAULT_TTL: Duration = Duration::from_secs(300);
const LONG_TTL: Duration = Duration::from_secs(3600);
const COLD_MARGIN: Duration = Duration::from_secs(30);
const LONG_TTL_MARKER: &[u8] = br#""ttl":"1h""#;

struct LaneCache {
    last_seen: Instant,
    ttl: Duration,
    cut: HashSet<String>,
}

static LANES: LazyLock<Mutex<HashMap<String, LaneCache>>> =
    LazyLock::new(|| Mutex::new(HashMap::new()));

static STARTED: LazyLock<Instant> = LazyLock::new(Instant::now);

pub(crate) struct CacheGate {
    lane: String,
    ttl: Duration,
    warm_cuts: Option<HashSet<String>>,
}

impl CacheGate {
    pub(crate) fn open(reducer: &'static str, raw: &[u8], body: &Value) -> Self {
        Self::open_at(reducer, raw, body, Instant::now())
    }

    fn open_at(reducer: &'static str, raw: &[u8], body: &Value, now: Instant) -> Self {
        let lane = lane_key(reducer, body);
        let ttl = request_ttl(raw);
        let started = *STARTED;
        let lanes = LANES.lock().unwrap_or_else(PoisonError::into_inner);
        let warm_cuts = match lanes.get(&lane) {
            Some(cache) if is_warm(cache, now) => Some(cache.cut.clone()),
            Some(_) => None,
            None if may_predate_process(started, now) => Some(HashSet::new()),
            None => None,
        };
        Self {
            lane,
            ttl,
            warm_cuts,
        }
    }

    pub(crate) fn warm_cuts(&self) -> Option<&HashSet<String>> {
        self.warm_cuts.as_ref()
    }

    #[cfg(test)]
    fn is_warm(&self) -> bool {
        self.warm_cuts.is_some()
    }

    pub(crate) fn close(self, applied: HashSet<String>) {
        self.close_at(applied, Instant::now());
    }

    fn close_at(self, applied: HashSet<String>, now: Instant) {
        let mut lanes = LANES.lock().unwrap_or_else(PoisonError::into_inner);
        lanes.retain(|_, cache| {
            now.saturating_duration_since(cache.last_seen) < LONG_TTL + COLD_MARGIN
        });
        lanes.insert(
            self.lane,
            LaneCache {
                last_seen: now,
                ttl: self.ttl,
                cut: applied,
            },
        );
    }
}

#[cfg(test)]
pub(crate) fn seed_cold_lane(reducer: &'static str, raw: &[u8]) {
    let body: Value = serde_json::from_slice(raw).unwrap_or(Value::Null);
    LANES.lock().unwrap_or_else(PoisonError::into_inner).insert(
        lane_key(reducer, &body),
        LaneCache {
            last_seen: Instant::now(),
            ttl: Duration::ZERO,
            cut: HashSet::new(),
        },
    );
}

fn may_predate_process(started: Instant, now: Instant) -> bool {
    now.saturating_duration_since(started) < LONG_TTL + COLD_MARGIN
}

fn is_warm(cache: &LaneCache, now: Instant) -> bool {
    now.saturating_duration_since(cache.last_seen) + COLD_MARGIN < cache.ttl
}

fn lane_key(reducer: &str, body: &Value) -> String {
    let first = body
        .get("messages")
        .and_then(Value::as_array)
        .and_then(|messages| messages.first());
    let identity = serde_json::json!([reducer, body.get("model"), body.get("system"), first]);
    sha256_hex(identity.to_string().as_bytes())
}

fn request_ttl(raw: &[u8]) -> Duration {
    if raw
        .windows(LONG_TTL_MARKER.len())
        .any(|window| window == LONG_TTL_MARKER)
    {
        LONG_TTL
    } else {
        DEFAULT_TTL
    }
}

#[cfg(test)]
#[path = "cache_gate_tests.rs"]
mod tests;
