use std::collections::HashMap;
use std::sync::Arc;

use arc_swap::ArcSwap;

use super::PromptClassKey;
use crate::wire::composition::Composition;

#[derive(Debug)]
pub struct CompositionCache {
    inner: ArcSwap<HashMap<PromptClassKey, Composition>>,
}

impl CompositionCache {
    pub fn new(initial: HashMap<PromptClassKey, Composition>) -> Self {
        Self {
            inner: ArcSwap::from_pointee(initial),
        }
    }

    pub fn empty() -> Self {
        Self::new(HashMap::new())
    }

    pub fn load(&self) -> Arc<HashMap<PromptClassKey, Composition>> {
        self.inner.load_full()
    }

    pub fn store(&self, new: HashMap<PromptClassKey, Composition>) {
        self.inner.store(Arc::new(new));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::composition::CompositionMode;
    use chrono::Utc;
    use serde_json::json;
    use uuid::Uuid;

    fn comp(prompt_sha256: &str, mode: CompositionMode) -> Composition {
        Composition {
            id: 0,
            uuid: Uuid::nil(),
            prompt_sha256: prompt_sha256.to_string(),
            label: "test".to_string(),
            blocks: vec![
                json!({"type": "text", "text": "b0"}),
                json!({"type": "text", "text": "b1"}),
                json!({"type": "text", "text": "b2"}),
            ],
            mode,
            notes: None,
            agent_id: None,
            agent_name: None,
            agent_definition_path: None,
            lane: "main".to_string(),
            version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        }
    }

    #[test]
    fn empty_cache_loads_empty_map() {
        let cache = CompositionCache::empty();
        let snap = cache.load();
        assert!(snap.is_empty());
    }

    #[test]
    fn seeded_cache_loads_with_entries() {
        let mut seed = HashMap::new();
        seed.insert(PromptClassKey::Main, comp("c1", CompositionMode::Observe));
        let cache = CompositionCache::new(seed);
        let snap = cache.load();
        assert_eq!(snap.len(), 1);
        assert!(snap.contains_key(&PromptClassKey::Main));
    }

    #[test]
    fn store_replaces_contents_atomically() {
        let cache = CompositionCache::empty();
        let mut next = HashMap::new();
        next.insert(
            PromptClassKey::Subagent("some-persona".to_string()),
            comp("c-new", CompositionMode::Template),
        );
        cache.store(next);
        let snap = cache.load();
        assert_eq!(snap.len(), 1);
        assert_eq!(
            snap[&PromptClassKey::Subagent("some-persona".to_string())].mode,
            CompositionMode::Template
        );
    }
}
