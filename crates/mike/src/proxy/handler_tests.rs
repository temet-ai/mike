mod global_cache_scope_guard {
    use crate::compose_prompt::ComposeOutcome;
    use crate::compose_prompt::ComposerService;
    use crate::compose_prompt::cache::CompositionCache;
    use crate::compose_prompt::volatile::VolatilePatternCache;
    use crate::filter::{
        self, FilterAction, FilterConfig, FilterKind, FilterOutcome, FilterRule, RuleSource,
    };
    use crate::lazy_sections::{self, LazySectionsOutcome};
    use crate::wire::composition::{Composition, CompositionMode};
    use crate::wire::prompt_classify::prompt_sha256_from_block_text;
    use chrono::Utc;
    use serde_json::json;
    use std::collections::HashMap;
    use std::sync::Arc;
    use uuid::Uuid;

    const HEADING: &str = "# Harness";
    const STUB: &str = "[harness details available on demand]";
    const INJECTED_APPEND: &str =
        "INJECTED-CONTENT-that-a-filter-append-rule-would-splice-in-right-after-block-1";
    const SECRET_SLICE: &str = "secret harness details";
    const ROLE_BODY_TEXT: &str =
        "role body\n# Harness\nsecret harness details\n# Next\nmore role text";

    fn system_body(scope_global: bool) -> serde_json::Value {
        let cache_control = if scope_global {
            json!({"type": "ephemeral", "ttl": "1h", "scope": "global"})
        } else {
            json!({"type": "ephemeral", "ttl": "1h"})
        };
        json!({
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 1024,
            "system": [
                {"type": "text", "text": "billing-header"},
                {"type": "text", "text": "identity preamble"},
                {
                    "type": "text",
                    "text": ROLE_BODY_TEXT,
                    "cache_control": cache_control,
                },
            ],
            "messages": [{"role": "user", "content": "hi"}]
        })
    }

    fn base_rule() -> FilterRule {
        FilterRule {
            id: 0,
            uuid: Uuid::new_v4(),
            kind: FilterKind::TextSubstring,
            value: String::new(),
            enabled: true,
            source: RuleSource::Ui,
            updated_at: Utc::now(),
            sort_order: 0,
            action: FilterAction::Replace,
            text: None,
            trigger_token: None,
            inject_text: None,
            project_path: None,
            style_override: false,
            title: None,
        }
    }

    fn slice_replace_rule() -> FilterRule {
        FilterRule {
            id: 1,
            kind: FilterKind::TextSubstring,
            value: SECRET_SLICE.to_string(),
            action: FilterAction::Replace,
            text: Some("MUTATED".to_string()),
            ..base_rule()
        }
    }

    fn lazy_section_rule() -> FilterRule {
        FilterRule {
            id: 2,
            kind: FilterKind::LazySection,
            value: HEADING.to_string(),
            text: Some(STUB.to_string()),
            trigger_token: Some("<chrome>".to_string()),
            ..base_rule()
        }
    }

    fn append_rule() -> FilterRule {
        FilterRule {
            id: 3,
            kind: FilterKind::TextSubstring,
            value: "identity preamble".to_string(),
            action: FilterAction::Append,
            text: Some(INJECTED_APPEND.to_string()),
            ..base_rule()
        }
    }

    fn composer_with_cached_template(block2_text: &str) -> ComposerService {
        let composition = Composition {
            id: 0,
            uuid: Uuid::nil(),
            prompt_sha256: prompt_sha256_from_block_text(block2_text),
            label: "main-agent".to_string(),
            blocks: vec![
                json!({"type": "text", "text": "billing-header"}),
                json!({"type": "text", "text": "identity preamble"}),
                json!({"type": "text", "text": "TEMPLATED-REPLACEMENT-BODY"}),
            ],
            mode: CompositionMode::Template,
            lane: "main".to_string(),
            notes: None,
            agent_id: None,
            agent_name: None,
            agent_definition_path: None,
            version: 1,
            created_at: Utc::now(),
            updated_at: Utc::now(),
        };
        let mut seed = HashMap::new();
        seed.insert(crate::compose_prompt::PromptClassKey::Main, composition);
        ComposerService::new(
            Arc::new(CompositionCache::new(seed)),
            Arc::new(VolatilePatternCache::empty()),
            true,
        )
    }

    fn apply_filter(raw: &[u8], cfg: &FilterConfig) -> Vec<u8> {
        match filter::apply_request_filter(raw, cfg, "") {
            FilterOutcome::Unchanged => raw.to_vec(),
            FilterOutcome::Rewrote { new_body, .. } => new_body.to_vec(),
        }
    }

    fn apply_lazy(raw: &[u8], cfg: &FilterConfig) -> Vec<u8> {
        match lazy_sections::apply_lazy_sections(raw, cfg, "") {
            LazySectionsOutcome::Unchanged => raw.to_vec(),
            LazySectionsOutcome::Rewrote { new_body, .. } => new_body.to_vec(),
        }
    }

    fn system_array(body: &[u8]) -> Vec<serde_json::Value> {
        serde_json::from_slice::<serde_json::Value>(body)
            .ok()
            .and_then(|v| {
                v.get("system")
                    .and_then(serde_json::Value::as_array)
                    .cloned()
            })
            .unwrap_or_default()
    }

    #[test]
    fn global_scope_prefix_survives_filter_lazy_and_composer() -> anyhow::Result<()> {
        let original = system_body(true);
        let raw = serde_json::to_vec(&original)?;
        let original_system = system_array(&raw);
        let cfg = FilterConfig {
            rules: vec![slice_replace_rule(), lazy_section_rule()],
        };
        let composer = composer_with_cached_template(ROLE_BODY_TEXT);

        let post_filter = apply_filter(&raw, &cfg);
        let post_lazy = apply_lazy(&post_filter, &cfg);
        let outcome = composer.classify_and_compose(
            &post_lazy,
            &raw,
            Some(&crate::compose_prompt::PromptClassKey::Main),
        );

        assert_eq!(
            post_lazy, raw,
            "no stage may touch a body whose only mutation targets are at/before the global block"
        );
        assert!(
            matches!(outcome, ComposeOutcome::Passthrough { .. }),
            "composer must fall back to Passthrough for a globally-scoped block 2, got {outcome:?}"
        );

        let forwarded_system = system_array(&post_lazy);
        crate::cache_scope::verify_global_prefix_preserved(&original_system, &forwarded_system)?;
        Ok(())
    }

    #[test]
    fn filter_never_inserts_a_block_ahead_of_the_global_prefix() -> anyhow::Result<()> {
        let raw = serde_json::to_vec(&system_body(true))?;
        let original_system = system_array(&raw);
        let cfg = FilterConfig {
            rules: vec![append_rule()],
        };

        let forwarded = apply_filter(&raw, &cfg);
        let forwarded_system = system_array(&forwarded);

        assert_eq!(
            forwarded_system.len(),
            original_system.len(),
            "an Append anchored before the global block must not fire at all; got {forwarded_system:?}"
        );
        crate::cache_scope::verify_global_prefix_preserved(&original_system, &forwarded_system)?;
        Ok(())
    }

    #[test]
    fn non_global_mutation_rules_still_fire() -> anyhow::Result<()> {
        let raw = serde_json::to_vec(&system_body(false))?;
        let cfg = FilterConfig {
            rules: vec![slice_replace_rule(), lazy_section_rule()],
        };
        let composer = composer_with_cached_template(ROLE_BODY_TEXT);

        let post_filter = apply_filter(&raw, &cfg);
        let post_lazy = apply_lazy(&post_filter, &cfg);
        let outcome = composer.classify_and_compose(
            &post_lazy,
            &raw,
            Some(&crate::compose_prompt::PromptClassKey::Main),
        );

        let forwarded_system = system_array(&post_lazy);
        assert!(
            !forwarded_system.iter().any(|b| b
                .get("text")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|t| t.contains(SECRET_SLICE))),
            "filter slice-replace must still fire without a global block; got {forwarded_system:?}"
        );
        assert!(
            forwarded_system.iter().any(|b| b
                .get("text")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|t| t.contains(STUB))),
            "lazy_sections heading-strip must still fire without a global block; got {forwarded_system:?}"
        );
        assert!(
            !matches!(outcome, ComposeOutcome::UnknownShape { .. }),
            "system[] shape must remain the canonical 3-block triple without an Append; got {outcome:?}"
        );
        Ok(())
    }

    #[test]
    fn non_global_append_rule_still_fires() -> anyhow::Result<()> {
        let raw = serde_json::to_vec(&system_body(false))?;
        let cfg = FilterConfig {
            rules: vec![append_rule()],
        };

        let forwarded = apply_filter(&raw, &cfg);
        let forwarded_system = system_array(&forwarded);

        assert!(
            forwarded_system
                .iter()
                .any(|b| b.get("text").and_then(serde_json::Value::as_str)
                    == Some(INJECTED_APPEND)),
            "filter append must still fire without a global block; got {forwarded_system:?}"
        );
        Ok(())
    }

    const TRAILING_TEXT: &str = "trailing-block-after-global";
    const TRAILING_MUTATED: &str = "MUTATED-TRAILING-BLOCK-post-global-splice";

    fn system_body_non_terminal_global() -> serde_json::Value {
        json!({
            "model": "claude-sonnet-4-20250514",
            "max_tokens": 1024,
            "system": [
                {"type": "text", "text": "billing-header"},
                {"type": "text", "text": "identity preamble"},
                {
                    "type": "text",
                    "text": ROLE_BODY_TEXT,
                    "cache_control": {"type": "ephemeral", "ttl": "1h", "scope": "global"},
                },
                {"type": "text", "text": TRAILING_TEXT},
            ],
            "messages": [{"role": "user", "content": "hi"}]
        })
    }

    fn trailing_replace_rule() -> FilterRule {
        FilterRule {
            id: 4,
            kind: FilterKind::TextSubstring,
            value: TRAILING_TEXT.to_string(),
            action: FilterAction::Replace,
            text: Some(TRAILING_MUTATED.to_string()),
            ..base_rule()
        }
    }

    #[test]
    fn non_terminal_global_prefix_protected_while_post_global_mutation_fires() -> anyhow::Result<()>
    {
        let raw = serde_json::to_vec(&system_body_non_terminal_global())?;
        let original_system = system_array(&raw);
        assert_eq!(
            crate::cache_scope::last_global_index(&original_system),
            Some(2),
            "fixture must place the global block at a non-terminal index"
        );

        let cfg = FilterConfig {
            rules: vec![
                slice_replace_rule(),
                lazy_section_rule(),
                trailing_replace_rule(),
            ],
        };
        let composer = composer_with_cached_template(ROLE_BODY_TEXT);

        let post_filter = apply_filter(&raw, &cfg);
        let post_lazy = apply_lazy(&post_filter, &cfg);
        let outcome = composer.classify_and_compose(
            &post_lazy,
            &raw,
            Some(&crate::compose_prompt::PromptClassKey::Main),
        );
        let forwarded_system = system_array(&post_lazy);

        crate::cache_scope::verify_global_prefix_preserved(&original_system, &forwarded_system)?;
        assert_eq!(
            crate::cache_scope::last_global_index(&forwarded_system),
            Some(2),
            "the global block must still sit at its original index; got {forwarded_system:?}"
        );

        let trailing_text = forwarded_system
            .get(3)
            .and_then(|b| b.get("text"))
            .and_then(serde_json::Value::as_str);
        assert_eq!(
            trailing_text,
            Some(TRAILING_MUTATED),
            "a filter Replace anchored after the global block must still fire; got {forwarded_system:?}"
        );

        assert!(
            matches!(outcome, ComposeOutcome::UnknownShape { .. }),
            "composer must not force a canonical decision on a 4-block system; got {outcome:?}"
        );
        Ok(())
    }
}
