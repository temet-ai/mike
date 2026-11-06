use std::collections::HashMap;
use std::sync::Arc;

use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

use super::cache::CompositionCache;
use super::classify::prompt_sha256_from_block_text;
use super::decision::{Composition, CompositionMode};
use super::volatile::VolatilePatternCache;
use super::{ComposerService, PromptClassKey};

pub(super) fn encode_body(body: &serde_json::Value) -> Vec<u8> {
    match serde_json::to_vec(body) {
        Ok(bytes) => bytes,
        Err(e) => panic!("test fixture serialization MUST be infallible: {e}; body = {body:?}"),
    }
}

pub(super) fn three_block_body() -> Vec<u8> {
    let body = json!({
        "model": "claude-sonnet-4-20250514",
        "max_tokens": 1024,
        "system": [
            {"type": "text", "text": "billing-header"},
            {"type": "text", "text": "identity preamble", "cache_control": {"type": "ephemeral"}},
            {"type": "text", "text": "role body", "cache_control": {"type": "ephemeral", "ttl": "1h"}},
        ],
        "messages": [{"role": "user", "content": "hi"}]
    });
    encode_body(&body)
}

pub(super) fn comp_for_role_body(text: &str, mode: CompositionMode) -> Composition {
    let prompt_sha256 = prompt_sha256_from_block_text("role body");
    Composition {
        id: 0,
        uuid: Uuid::nil(),
        prompt_sha256,
        label: "main-agent".to_string(),
        blocks: vec![
            json!({"type": "text", "text": "billing-header"}),
            json!({"type": "text", "text": "identity preamble"}),
            json!({"type": "text", "text": text}),
        ],
        mode,
        lane: "main".to_string(),
        notes: None,
        agent_id: None,
        agent_name: None,
        agent_definition_path: None,
        version: 1,
        created_at: Utc::now(),
        updated_at: Utc::now(),
    }
}

pub(super) fn test_class() -> PromptClassKey {
    PromptClassKey::Main
}

pub(super) fn cache_with(comp: Composition) -> Arc<CompositionCache> {
    let mut seed = HashMap::new();
    seed.insert(test_class(), comp);
    Arc::new(CompositionCache::new(seed))
}

pub(super) fn svc(cache: Arc<CompositionCache>, enabled: bool) -> ComposerService {
    ComposerService::new(cache, Arc::new(VolatilePatternCache::empty()), enabled)
}
