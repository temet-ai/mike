use super::test_support::{
    cache_with, comp_for_role_body, encode_body, svc, test_class, three_block_body,
};
use super::*;
use decision::CompositionMode;
use serde_json::json;
use template::find_subsequence;

#[test]
fn template_substitutes_block_2_preserving_cache_control() -> anyhow::Result<()> {
    let comp = comp_for_role_body("REWRITTEN-BODY", CompositionMode::Template);
    let cache = cache_with(comp);
    let svc = svc(cache, true);
    let body = three_block_body();
    let outcome = svc.classify_and_compose(&body, &body, Some(&test_class()));
    let (new_body, label) = match outcome {
        ComposeOutcome::Templated {
            new_body, label, ..
        } => (new_body, label),
        other => return Err(anyhow::anyhow!("expected Templated, got {other:?}")),
    };
    assert_eq!(
        label, "main-agent",
        "Templated outcome must carry the matched composition's label"
    );
    let parsed: serde_json::Value = serde_json::from_slice(&new_body)?;
    assert_eq!(parsed["system"][0]["text"], "billing-header");
    assert_eq!(parsed["system"][1]["cache_control"]["type"], "ephemeral");
    assert_eq!(parsed["system"][2]["text"], "REWRITTEN-BODY");
    assert_eq!(parsed["system"][2]["cache_control"]["type"], "ephemeral");
    assert_eq!(parsed["system"][2]["cache_control"]["ttl"], "1h");
    Ok(())
}

#[test]
fn classify_and_compose_never_templates_a_globally_scoped_block2() -> anyhow::Result<()> {
    let comp = comp_for_role_body("REWRITTEN-BODY", CompositionMode::Template);
    let cache = cache_with(comp);
    let svc = svc(cache, true);
    let body = json!({
        "model": "claude-sonnet-4-20250514",
        "max_tokens": 1024,
        "system": [
            {"type": "text", "text": "billing-header"},
            {"type": "text", "text": "identity preamble", "cache_control": {"type": "ephemeral"}},
            {"type": "text", "text": "role body", "cache_control": {"type": "ephemeral", "ttl": "1h", "scope": "global"}},
        ],
        "messages": [{"role": "user", "content": "hi"}]
    });
    let raw = encode_body(&body);
    let outcome = svc.classify_and_compose(&raw, &raw, Some(&test_class()));
    match outcome {
        ComposeOutcome::Passthrough { prompt_sha256 } => {
            let expected = prompt_sha256_from_block_text("role body");
            assert_eq!(
                prompt_sha256.as_deref(),
                Some(expected.as_str()),
                "identity must still be stamped even though templating is skipped"
            );
        }
        other => return Err(anyhow::anyhow!("expected Passthrough, got {other:?}")),
    }
    Ok(())
}

#[test]
fn template_preserves_bytes_outside_block_2_text() -> anyhow::Result<()> {
    let comp = comp_for_role_body("REWRITTEN-BODY", CompositionMode::Template);
    let cache = cache_with(comp);
    let svc = svc(cache, true);
    let original_body = three_block_body();
    let outcome = svc.classify_and_compose(&original_body, &original_body, Some(&test_class()));
    let new_body = match outcome {
        ComposeOutcome::Templated { new_body, .. } => new_body,
        other => return Err(anyhow::anyhow!("expected Templated, got {other:?}")),
    };

    let original_encoded = serde_json::to_vec(&"role body")?;
    let new_encoded = serde_json::to_vec(&"REWRITTEN-BODY")?;

    let pre_len = find_subsequence(&original_body, &original_encoded)
        .ok_or_else(|| anyhow::anyhow!("setup: original body should contain encoded text"))?;

    assert_eq!(
        &new_body[..pre_len],
        &original_body[..pre_len],
        "bytes BEFORE the splice point must be verbatim"
    );

    assert_eq!(
        &new_body[pre_len..pre_len + new_encoded.len()],
        new_encoded.as_slice(),
        "spliced range must contain the composition's encoded text"
    );

    let new_post_offset = pre_len + new_encoded.len();
    let original_post_offset = pre_len + original_encoded.len();
    assert_eq!(
        &new_body[new_post_offset..],
        &original_body[original_post_offset..],
        "bytes AFTER the splice point must be verbatim"
    );
    Ok(())
}

#[test]
fn template_falls_back_when_block_2_text_is_ambiguous() {
    let comp = comp_for_role_body("REWRITTEN", CompositionMode::Template);
    let cache = cache_with(comp);
    let svc = svc(cache, true);

    let body = json!({
        "system": [
            {"type": "text", "text": "billing-header"},
            {"type": "text", "text": "identity preamble"},
            {"type": "text", "text": "role body"},
        ],
        "metadata": {"label": "role body"},
        "messages": [{"role": "user", "content": "hi"}]
    });
    let raw = encode_body(&body);
    match svc.classify_and_compose(&raw, &raw, Some(&test_class())) {
        ComposeOutcome::Passthrough { prompt_sha256 } => {
            let expected = prompt_sha256_from_block_text("role body");
            assert_eq!(prompt_sha256.as_deref(), Some(expected.as_str()));
        }
        other => panic!("expected Passthrough on ambiguous splice, got {other:?}"),
    }
}

#[test]
fn composition_disabled_globally_is_passthrough() {
    let comp = comp_for_role_body("REWRITTEN", CompositionMode::Template);
    let cache = cache_with(comp);
    let svc = svc(cache, false);
    let body = three_block_body();
    match svc.classify_and_compose(&body, &body, Some(&test_class())) {
        ComposeOutcome::Passthrough { prompt_sha256 } => assert!(
            prompt_sha256.is_none(),
            "kill switch must NOT leak a sha; full silence"
        ),
        other => panic!("expected Passthrough on disabled, got {other:?}"),
    }
}

#[test]
fn replace_mode_falls_back_to_replace_unsupported_in_v1() {
    let comp = comp_for_role_body("X", CompositionMode::Replace);
    let cache = cache_with(comp);
    let svc = svc(cache, true);
    let body = three_block_body();
    match svc.classify_and_compose(&body, &body, Some(&test_class())) {
        ComposeOutcome::ReplaceUnsupported { prompt_sha256 } => {
            let expected = prompt_sha256_from_block_text("role body");
            assert_eq!(prompt_sha256, expected);
        }
        other => panic!("expected ReplaceUnsupported, got {other:?}"),
    }
}
