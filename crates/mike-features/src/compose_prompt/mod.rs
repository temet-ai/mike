pub mod cache;
pub mod class_key;
pub mod eligibility;
pub mod stage;
pub mod store;
pub mod sub_patterns;
pub mod template;
pub mod volatile;

#[cfg(test)]
pub mod template_splice_tests;
#[cfg(test)]
pub mod test_support;
#[cfg(test)]
mod tests;

use std::sync::Arc;

use bytes::Bytes;
use serde_json::Value;

use crate::wire::composition::{Composition, Decision, DecisionInputs, decide};
use crate::wire::prompt_blocks::{
    ROLE_BODY_STRIP_INDEX, parse_identity_blocks, parse_system_blocks,
};
use crate::wire::prompt_classify::prompt_sha256_from_block_text;
use cache::CompositionCache;
pub use class_key::PromptClassKey;
use template::compose_template;
use volatile::{VolatilePatternCache, strip_volatile_lines};

#[derive(Debug)]
pub enum ComposeOutcome {
    Passthrough {
        prompt_sha256: Option<String>,
    },
    Templated {
        prompt_sha256: String,
        composition_id: i64,
        label: String,
        new_body: Bytes,
    },
    UnknownShape {
        unknown_blocks: Value,
        prompt_sha256: Option<String>,
    },
    ReplaceUnsupported {
        prompt_sha256: String,
    },
    SpliceTargetMissing {
        prompt_sha256: String,
        composition_id: i64,
    },
}

#[derive(Debug)]
pub struct ComposerService {
    cache: Arc<CompositionCache>,
    volatile_cache: Arc<VolatilePatternCache>,
    enabled: bool,
}

impl ComposerService {
    pub fn new(
        cache: Arc<CompositionCache>,
        volatile_cache: Arc<VolatilePatternCache>,
        enabled: bool,
    ) -> Self {
        Self {
            cache,
            volatile_cache,
            enabled,
        }
    }

    pub fn cache(&self) -> Arc<CompositionCache> {
        self.cache.clone()
    }

    pub fn volatile_cache(&self) -> Arc<VolatilePatternCache> {
        self.volatile_cache.clone()
    }

    pub fn classify_and_compose(
        &self,
        raw_body: &[u8],
        pre_filter_body: &[u8],
        class_key: Option<&PromptClassKey>,
    ) -> ComposeOutcome {
        if !self.enabled {
            return ComposeOutcome::Passthrough {
                prompt_sha256: None,
            };
        }

        let body: Value = match serde_json::from_slice(pre_filter_body) {
            Ok(v) => v,
            Err(_) => {
                return ComposeOutcome::Passthrough {
                    prompt_sha256: None,
                };
            }
        };

        let parse = parse_system_blocks(&body);
        let cache_snap = self.cache.load();
        let volatile_snap = self.volatile_cache.load();

        let computed_prompt_sha256 = identity_sha256(&body, &volatile_snap);
        let parse_result_for_decide = match &parse {
            Ok(triple) => Ok(triple.block2_text.as_str()),
            Err(reason) => Err(reason),
        };

        let raw_value: Option<Value> = if parse.is_ok() {
            serde_json::from_slice(raw_body).ok()
        } else {
            None
        };

        if raw_value
            .as_ref()
            .is_some_and(crate::wire::prompt_blocks::block2_is_globally_scoped)
        {
            return ComposeOutcome::Passthrough {
                prompt_sha256: computed_prompt_sha256,
            };
        }

        let lookup: Option<&Composition> = class_key.and_then(|key| cache_snap.get(key));

        let decision = decide(
            DecisionInputs {
                enabled: self.enabled,
                parse_result: parse_result_for_decide,
                lookup,
            },
            computed_prompt_sha256.clone(),
        );

        resolve_composed_outcome(
            decision,
            computed_prompt_sha256,
            raw_value.as_ref(),
            lookup,
            raw_body,
        )
    }

    pub fn observe_prompt_identity(&self, raw_body: &[u8]) -> Option<String> {
        if !self.enabled {
            return None;
        }
        let body: Value = serde_json::from_slice(raw_body).ok()?;
        let volatile_snap = self.volatile_cache.load();
        identity_sha256(&body, &volatile_snap)
    }
}

pub fn identity_sha256(
    body: &Value,
    volatile_patterns: &[crate::compose_prompt::volatile::VolatilePattern],
) -> Option<String> {
    let identity = parse_identity_blocks(body).ok()?;
    let stripped = strip_volatile_lines(
        &identity.role_body_text,
        volatile_patterns,
        ROLE_BODY_STRIP_INDEX,
    );
    Some(prompt_sha256_from_block_text(&stripped))
}

fn resolve_composed_outcome(
    decision: Decision,
    computed_prompt_sha256: Option<String>,
    raw_value: Option<&Value>,
    lookup: Option<&Composition>,
    raw_body: &[u8],
) -> ComposeOutcome {
    match decision {
        Decision::Passthrough { prompt_sha256 } => ComposeOutcome::Passthrough { prompt_sha256 },
        Decision::UnknownShape { unknown_blocks } => ComposeOutcome::UnknownShape {
            unknown_blocks,
            prompt_sha256: computed_prompt_sha256,
        },
        Decision::Templated {
            prompt_sha256,
            composition_id,
        } => resolve_templated_outcome(prompt_sha256, composition_id, raw_value, lookup, raw_body),
        Decision::Replace { prompt_sha256 } => {
            tracing::error!(
                target: "mike::compose_prompt",
                prompt_sha256 = %prompt_sha256,
                "composer mode=replace requested but not enabled in V1; \
                 falling back to passthrough (replace_unsupported)"
            );
            ComposeOutcome::ReplaceUnsupported { prompt_sha256 }
        }
    }
}

fn resolve_templated_outcome(
    prompt_sha256: String,
    composition_id: i64,
    raw_value: Option<&Value>,
    lookup: Option<&Composition>,
    raw_body: &[u8],
) -> ComposeOutcome {
    let triple = match raw_value.and_then(|v| parse_system_blocks(v).ok()) {
        Some(t) => t,
        None => {
            return ComposeOutcome::SpliceTargetMissing {
                prompt_sha256,
                composition_id,
            };
        }
    };
    let composition = match lookup {
        Some(c) => c,
        None => {
            return ComposeOutcome::Passthrough {
                prompt_sha256: None,
            };
        }
    };
    match compose_template(raw_body, &triple, composition) {
        Ok(new_body) => ComposeOutcome::Templated {
            prompt_sha256,
            composition_id,
            label: composition.label.clone(),
            new_body,
        },
        Err(err) => {
            tracing::warn!(
                prompt_sha256 = %prompt_sha256,
                error = %err,
                "composer template splice failed; falling back to passthrough"
            );
            ComposeOutcome::Passthrough {
                prompt_sha256: Some(prompt_sha256),
            }
        }
    }
}
