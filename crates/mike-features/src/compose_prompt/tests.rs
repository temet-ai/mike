use super::test_support::{encode_body, svc, test_class, three_block_body};
use super::*;
use crate::filter::{
    self, FilterAction, FilterConfig, FilterKind, FilterOutcome, FilterRule, RuleSource,
};
use chrono::Utc;
use serde_json::json;
use uuid::Uuid;

#[test]
fn classifies_main_agent_from_block_2_text() {
    let svc = svc(Arc::new(CompositionCache::empty()), true);
    let body = three_block_body();
    let outcome = svc.classify_and_compose(&body, &body, Some(&test_class()));
    match outcome {
        ComposeOutcome::Passthrough { prompt_sha256 } => {
            let expected = prompt_sha256_from_block_text("role body");
            assert_eq!(prompt_sha256.as_deref(), Some(expected.as_str()));
        }
        other => panic!("expected Passthrough with prompt_sha256, got {other:?}"),
    }
}

#[test]
fn observe_prompt_identity_matches_classifier_sha() {
    let svc = svc(Arc::new(CompositionCache::empty()), true);
    let body = three_block_body();

    let classifier_sha = match svc.classify_and_compose(&body, &body, Some(&test_class())) {
        ComposeOutcome::Passthrough { prompt_sha256 } => prompt_sha256,
        other => panic!("expected Passthrough with prompt_sha256, got {other:?}"),
    };

    let observed_sha = svc.observe_prompt_identity(&body);

    assert_eq!(
        observed_sha, classifier_sha,
        "capture-only identity must equal the classifier sha byte-for-byte; \
         observed={observed_sha:?} classifier={classifier_sha:?}"
    );
    assert!(
        observed_sha.is_some(),
        "a canonical 3-block body must yield a stable identity"
    );
}

#[test]
fn observe_prompt_identity_none_without_system() {
    let svc = svc(Arc::new(CompositionCache::empty()), true);
    let body = encode_body(&json!({
        "model": "claude-haiku-4-5-20251001",
        "max_tokens": 1,
        "messages": [{"role": "user", "content": "quota"}]
    }));
    assert_eq!(svc.observe_prompt_identity(&body), None);
}

#[test]
fn observe_prompt_identity_none_when_disabled() {
    let svc = svc(Arc::new(CompositionCache::empty()), false);
    assert_eq!(svc.observe_prompt_identity(&three_block_body()), None);
}

#[test]
fn passthrough_when_class_unknown() {
    let svc = svc(Arc::new(CompositionCache::empty()), true);
    let body = json!({
        "system": [
            {"type": "text", "text": "billing"},
            {"type": "text", "text": "preamble"},
            {"type": "text", "text": "completely novel string"},
        ]
    });
    let raw = encode_body(&body);
    match svc.classify_and_compose(&raw, &raw, Some(&test_class())) {
        ComposeOutcome::Passthrough { prompt_sha256 } => {
            let expected = prompt_sha256_from_block_text("completely novel string");
            assert_eq!(prompt_sha256.as_deref(), Some(expected.as_str()));
        }
        other => panic!("expected Passthrough, got {other:?}"),
    }
}

#[test]
fn passthrough_on_string_form_system() {
    let svc = svc(Arc::new(CompositionCache::empty()), true);
    let body = json!({"system": "hello"});
    let raw = encode_body(&body);
    match svc.classify_and_compose(&raw, &raw, Some(&test_class())) {
        ComposeOutcome::UnknownShape {
            unknown_blocks,
            prompt_sha256,
        } => {
            assert_eq!(unknown_blocks, json!({"reason": "string_form"}));
            assert_eq!(prompt_sha256, Some(prompt_sha256_from_block_text("hello")));
        }
        other => panic!("expected UnknownShape, got {other:?}"),
    }
}

#[test]
fn passthrough_on_non_3_block_shape() {
    let svc = svc(Arc::new(CompositionCache::empty()), true);
    for (system, expected_len) in [
        (json!([]), 0),
        (json!([{"type": "text", "text": "a"}]), 1),
        (
            json!([
                {"type": "text", "text": "a"},
                {"type": "text", "text": "b"},
            ]),
            2,
        ),
        (
            json!([
                {"type": "text", "text": "a"},
                {"type": "text", "text": "b"},
                {"type": "text", "text": "c"},
                {"type": "text", "text": "d"},
            ]),
            4,
        ),
    ] {
        let body = json!({"system": system});
        let raw = encode_body(&body);
        match svc.classify_and_compose(&raw, &raw, Some(&test_class())) {
            ComposeOutcome::UnknownShape {
                unknown_blocks,
                prompt_sha256,
            } => {
                assert_eq!(
                    unknown_blocks,
                    json!({"reason": "non_3_block", "len": expected_len})
                );
                assert_eq!(prompt_sha256.is_some(), expected_len > 0);
            }
            other => panic!("len={expected_len}: expected UnknownShape, got {other:?}"),
        }
    }
}

#[test]
fn prompt_sha256_matches_sha256_hex_of_block_2_text() {
    let svc = svc(Arc::new(CompositionCache::empty()), true);
    let body = three_block_body();
    let outcome = svc.classify_and_compose(&body, &body, Some(&test_class()));
    let id = match outcome {
        ComposeOutcome::Passthrough {
            prompt_sha256: Some(id),
        } => id,
        other => panic!("expected classified Passthrough, got {other:?}"),
    };
    assert_eq!(id.len(), 64);
    assert!(id.chars().all(|c| c.is_ascii_hexdigit()));
    use sha2::Digest;
    let mut hasher = sha2::Sha256::new();
    hasher.update(b"role body");
    let expected = hex::encode(hasher.finalize());
    assert_eq!(id, expected);
}

#[test]
fn malformed_json_body_returns_passthrough() {
    let svc = svc(Arc::new(CompositionCache::empty()), true);
    let raw = b"not json";
    match svc.classify_and_compose(raw, raw, Some(&test_class())) {
        ComposeOutcome::Passthrough { prompt_sha256 } => assert!(prompt_sha256.is_none()),
        other => panic!("expected Passthrough on bad JSON, got {other:?}"),
    }
}

#[test]
fn classify_and_compose_shas_pre_filter_text_not_filter_rewritten_text() {
    let pre_filter_body = three_block_body();

    let cfg = FilterConfig {
        rules: vec![FilterRule {
            id: 1,
            uuid: Uuid::new_v4(),
            kind: FilterKind::TextSubstring,
            value: "role body".to_string(),
            enabled: true,
            source: RuleSource::Ui,
            updated_at: Utc::now(),
            sort_order: 0,
            action: FilterAction::Replace,
            text: Some("REWRITTEN-BY-FILTER".to_string()),
            trigger_token: None,
            inject_text: None,
            project_path: None,
            style_override: false,
            title: None,
        }],
    };
    let post_filter_body = match filter::apply_request_filter(&pre_filter_body, &cfg, "") {
        FilterOutcome::Rewrote { new_body, .. } => new_body.to_vec(),
        FilterOutcome::Unchanged => {
            panic!("setup: filter rule must fire to exercise a real rewrite")
        }
    };

    let svc = svc(Arc::new(CompositionCache::empty()), true);
    let outcome =
        svc.classify_and_compose(&post_filter_body, &pre_filter_body, Some(&test_class()));
    let prompt_sha256 = match outcome {
        ComposeOutcome::Passthrough { prompt_sha256 } => prompt_sha256,
        other => panic!("expected Passthrough (no registered composition), got {other:?}"),
    };

    let pre_filter_sha = prompt_sha256_from_block_text("role body");
    let post_filter_sha = prompt_sha256_from_block_text("REWRITTEN-BY-FILTER");
    assert_ne!(
        pre_filter_sha, post_filter_sha,
        "setup: the filter rewrite must actually change block 2's text"
    );
    assert_eq!(
        prompt_sha256.as_deref(),
        Some(pre_filter_sha.as_str()),
        "composer must classify against the client's pre-filter text"
    );
    assert_ne!(
        prompt_sha256.as_deref(),
        Some(post_filter_sha.as_str()),
        "composer must NOT classify against the filter-rewritten text"
    );
}

#[test]
fn classify_and_compose_unchanged_when_no_filter_rule_fires() {
    let pre_filter_body = three_block_body();
    let cfg = FilterConfig { rules: vec![] };
    let post_filter_body = match filter::apply_request_filter(&pre_filter_body, &cfg, "") {
        FilterOutcome::Unchanged => pre_filter_body.clone(),
        FilterOutcome::Rewrote { .. } => {
            panic!("setup: no rules registered, nothing should fire")
        }
    };
    assert_eq!(
        post_filter_body, pre_filter_body,
        "setup: with no rule firing, pre- and post-filter bodies must be identical"
    );

    let svc = svc(Arc::new(CompositionCache::empty()), true);
    match svc.classify_and_compose(&post_filter_body, &pre_filter_body, Some(&test_class())) {
        ComposeOutcome::Passthrough { prompt_sha256 } => {
            assert_eq!(
                prompt_sha256.as_deref(),
                Some(prompt_sha256_from_block_text("role body").as_str())
            );
        }
        other => panic!("expected Passthrough, got {other:?}"),
    }
}
