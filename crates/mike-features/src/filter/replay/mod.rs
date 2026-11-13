#[cfg(test)]
pub mod tests;

use std::collections::{HashMap, VecDeque};
use std::hash::{Hash, Hasher};
use std::sync::{Arc, Mutex};

use bytes::Bytes;
use dashmap::DashMap;
use serde::Deserialize;
use serde_json::value::RawValue;

const DEFAULT_CAPACITY: usize = 256;
const BILLING_HEADER: &str = "x-anthropic-billing-header";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct MutableRegion {
    start: usize,
    end: usize,
    boundary: Option<usize>,
}

#[derive(Deserialize)]
struct BlockScope<'a> {
    #[serde(borrow, default)]
    cache_control: Option<CacheControl<'a>>,
}

#[derive(Deserialize)]
struct CacheControl<'a> {
    #[serde(borrow, default)]
    scope: Option<&'a str>,
}

fn is_globally_scoped(block: &RawValue) -> bool {
    serde_json::from_str::<BlockScope<'_>>(block.get())
        .ok()
        .and_then(|b| b.cache_control.and_then(|cc| cc.scope))
        == Some("global")
}

#[derive(Deserialize)]
struct BlockText<'a> {
    #[serde(borrow, default)]
    text: Option<std::borrow::Cow<'a, str>>,
}

fn is_billing_header(block: &RawValue) -> bool {
    serde_json::from_str::<BlockText<'_>>(block.get())
        .ok()
        .and_then(|b| b.text)
        .is_some_and(|text| text.starts_with(BILLING_HEADER))
}

fn protected_prefix(blocks: &[&RawValue]) -> Option<usize> {
    blocks
        .iter()
        .rposition(|b| is_globally_scoped(b))
        .or_else(|| blocks.first().filter(|b| is_billing_header(b)).map(|_| 0))
}

fn locate_mutable_region(raw: &[u8]) -> Option<MutableRegion> {
    let top: HashMap<&str, &RawValue> = serde_json::from_slice(raw).ok()?;
    let system = top.get("system")?;
    let blocks: Vec<&RawValue> = serde_json::from_str(system.get()).ok()?;
    let last = blocks.len().checked_sub(1)?;

    let base = raw.as_ptr() as usize;
    let span = |rv: &RawValue| {
        let text = rv.get();
        let start = text.as_ptr() as usize - base;
        (start, start + text.len())
    };

    let boundary = protected_prefix(&blocks);
    let (start, end) = match boundary {
        Some(b) if b == last => {
            let e = span(blocks[b]).1;
            (e, e)
        }
        Some(b) => (span(blocks[b]).1, span(blocks[last]).1),
        None => (span(blocks[0]).0, span(blocks[last]).1),
    };
    Some(MutableRegion {
        start,
        end,
        boundary,
    })
}

pub fn ruleset_fingerprint(cfg: &super::FilterConfig, project_path: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    for rule in cfg.rules.iter().filter(|r| {
        r.enabled
            && r.project_path
                .as_deref()
                .is_none_or(|p| super::project_scope_matches(p, project_path))
    }) {
        rule.id.hash(&mut hasher);
        rule.updated_at.hash(&mut hasher);
    }
    hasher.finish()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SystemReplay {
    source: Vec<u8>,
    replacement: Vec<u8>,
    firing_rule_ids: Vec<i64>,
    fingerprint: u64,
}

impl SystemReplay {
    pub fn capture(
        original: &[u8],
        filtered: &[u8],
        firing_rule_ids: &[i64],
        fingerprint: u64,
    ) -> Option<Self> {
        let o = locate_mutable_region(original)?;
        let f = locate_mutable_region(filtered)?;
        if o.boundary != f.boundary {
            return None;
        }
        let source = original.get(o.start..o.end)?.to_vec();
        let replacement = filtered.get(f.start..f.end)?.to_vec();
        if source == replacement {
            return None;
        }
        Some(Self {
            source,
            replacement,
            firing_rule_ids: firing_rule_ids.to_vec(),
            fingerprint,
        })
    }

    pub fn firing_rule_ids(&self) -> &[i64] {
        &self.firing_rule_ids
    }

    pub fn matches_ruleset(&self, fingerprint: u64) -> bool {
        self.fingerprint == fingerprint
    }

    pub fn apply(&self, raw: &[u8]) -> Option<Bytes> {
        let region = locate_mutable_region(raw)?;
        if raw.get(region.start..region.end)? != self.source.as_slice() {
            return None;
        }
        let mut out = Vec::with_capacity(raw.len() - self.source.len() + self.replacement.len());
        out.extend_from_slice(&raw[..region.start]);
        out.extend_from_slice(&self.replacement);
        out.extend_from_slice(&raw[region.end..]);
        Some(Bytes::from(out))
    }
}

#[derive(Debug, Default)]
pub struct ReplayRegistry {
    unfiltered: DashMap<String, ()>,
    entries: DashMap<String, Arc<SystemReplay>>,
    order: Mutex<VecDeque<String>>,
    capacity: usize,
}

impl ReplayRegistry {
    pub fn new() -> Self {
        Self::with_capacity(DEFAULT_CAPACITY)
    }

    pub fn with_capacity(capacity: usize) -> Self {
        Self {
            unfiltered: DashMap::new(),
            entries: DashMap::new(),
            order: Mutex::new(VecDeque::new()),
            capacity: capacity.max(1),
        }
    }

    pub fn is_unfiltered(&self, session_id: &str) -> bool {
        self.unfiltered.contains_key(session_id)
    }

    pub fn mark_unfiltered(&self, session_id: &str) {
        if self.unfiltered.len() >= self.capacity {
            self.unfiltered.clear();
        }
        self.unfiltered.insert(session_id.to_string(), ());
    }

    pub fn has(&self, session_id: &str) -> bool {
        self.entries.contains_key(session_id)
    }

    pub fn get(&self, session_id: &str) -> Option<Arc<SystemReplay>> {
        self.entries.get(session_id).map(|e| Arc::clone(e.value()))
    }

    pub fn insert(&self, session_id: &str, replay: SystemReplay) {
        self.unfiltered.remove(session_id);
        let is_new = self
            .entries
            .insert(session_id.to_string(), Arc::new(replay))
            .is_none();
        let mut order = match self.order.lock() {
            Ok(o) => o,
            Err(poisoned) => poisoned.into_inner(),
        };
        if is_new {
            order.push_back(session_id.to_string());
        }
        while order.len() > self.capacity {
            if let Some(oldest) = order.pop_front() {
                self.entries.remove(&oldest);
            }
        }
    }

    #[cfg(any(test, feature = "test-seams"))]
    pub fn len(&self) -> usize {
        self.entries.len()
    }
}
