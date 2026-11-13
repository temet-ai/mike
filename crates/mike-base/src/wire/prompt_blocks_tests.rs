use super::*;
use serde_json::{Value, json};

fn three_block_body() -> Value {
    json!({
        "system": [
            {"type": "text", "text": "billing-header"},
            {"type": "text", "text": "identity preamble", "cache_control": {"type": "ephemeral"}},
            {"type": "text", "text": "role body", "cache_control": {"type": "ephemeral", "ttl": "1h"}},
        ]
    })
}

#[test]
fn parse_canonical_three_block_shape_extracts_block2_text() -> anyhow::Result<()> {
    let body = three_block_body();
    let triple =
        parse_system_blocks(&body).map_err(|r| anyhow::anyhow!("expected Ok, got {r:?}"))?;
    assert_eq!(triple.block2_text, "role body");
    Ok(())
}

fn err_of(body: &Value) -> anyhow::Result<UnknownShapeReason> {
    match parse_system_blocks(body) {
        Ok(triple) => Err(anyhow::anyhow!("expected Err, got Ok({:?})", triple)),
        Err(reason) => Ok(reason),
    }
}

fn ident_of(body: &Value) -> anyhow::Result<IdentityBlocks> {
    parse_identity_blocks(body).map_err(|r| anyhow::anyhow!("expected Ok, got {r:?}"))
}

fn ident_err(body: &Value) -> anyhow::Result<UnknownShapeReason> {
    match parse_identity_blocks(body) {
        Ok(id) => Err(anyhow::anyhow!("expected Err, got Ok({id:?})")),
        Err(reason) => Ok(reason),
    }
}

#[test]
fn identity_agrees_with_canonical_parser_on_the_three_block_shape() -> anyhow::Result<()> {
    let body = three_block_body();
    let triple =
        parse_system_blocks(&body).map_err(|r| anyhow::anyhow!("expected Ok, got {r:?}"))?;
    assert_eq!(ident_of(&body)?.role_body_text, triple.block2_text);
    Ok(())
}

#[test]
fn identity_resolves_the_four_block_interactive_shape() -> anyhow::Result<()> {
    let body = json!({
        "system": [
            {"type": "text", "text": "x-anthropic-billing-header: cc_version=2.1.237.f50; cch=dbbda;"},
            {"type": "text", "text": "You are Claude Code, Anthropic's official CLI for Claude."},
            {"type": "text", "text": "harness prompt", "cache_control": {"type": "ephemeral"}},
            {"type": "text", "text": "role body", "cache_control": {"type": "ephemeral"}},
        ]
    });
    assert_eq!(err_of(&body)?, UnknownShapeReason::NonThreeBlock { len: 4 });
    assert_eq!(ident_of(&body)?.role_body_text, "role body");
    Ok(())
}

#[test]
fn identity_resolves_the_two_block_sdk_shape() -> anyhow::Result<()> {
    let body = json!({
        "system": [
            {"type": "text", "text": "x-anthropic-billing-header: cc_entrypoint=sdk-cli;"},
            {"type": "text", "text": "You are a Claude agent, built on Anthropic's Claude Agent SDK."},
        ]
    });
    assert_eq!(
        ident_of(&body)?.role_body_text,
        "You are a Claude agent, built on Anthropic's Claude Agent SDK."
    );
    Ok(())
}

#[test]
fn identity_resolves_string_form_system() -> anyhow::Result<()> {
    let body = json!({"system": "you are a helpful assistant"});
    assert_eq!(
        ident_of(&body)?.role_body_text,
        "you are a helpful assistant"
    );
    Ok(())
}

#[test]
fn identity_without_a_billing_header_keeps_every_block() -> anyhow::Result<()> {
    let body = json!({"system": [{"type": "text", "text": "sole block"}]});
    assert_eq!(ident_of(&body)?.role_body_text, "sole block");
    Ok(())
}

#[test]
fn identity_rejects_a_header_only_array() -> anyhow::Result<()> {
    let body = json!({
        "system": [{"type": "text", "text": "x-anthropic-billing-header: cch=abc;"}]
    });
    assert_eq!(
        ident_err(&body)?,
        UnknownShapeReason::NoIdentityBlock { len: 1 }
    );
    Ok(())
}

#[test]
fn identity_rejects_an_empty_array_and_non_text_blocks() -> anyhow::Result<()> {
    let empty = json!({"system": []});
    assert_eq!(
        ident_err(&empty)?,
        UnknownShapeReason::NoIdentityBlock { len: 0 }
    );
    let image = json!({
        "system": [
            {"type": "text", "text": "x-anthropic-billing-header: cch=abc;"},
            {"type": "image", "source": {}},
        ]
    });
    assert_eq!(
        ident_err(&image)?,
        UnknownShapeReason::NonTextBlock { index: 1 }
    );
    Ok(())
}

#[test]
fn parse_missing_system_field_returns_missing() -> anyhow::Result<()> {
    let body = json!({"messages": []});
    assert_eq!(err_of(&body)?, UnknownShapeReason::Missing);
    Ok(())
}

#[test]
fn parse_null_system_field_returns_missing() -> anyhow::Result<()> {
    let body = json!({"system": null, "messages": []});
    assert_eq!(err_of(&body)?, UnknownShapeReason::Missing);
    Ok(())
}

#[test]
fn parse_string_system_returns_string_form() -> anyhow::Result<()> {
    let body = json!({"system": "you are a helpful assistant"});
    assert_eq!(err_of(&body)?, UnknownShapeReason::StringForm);
    Ok(())
}

#[test]
fn parse_non_array_non_string_system_returns_missing() -> anyhow::Result<()> {
    let body = json!({"system": {"text": "what"}});
    assert_eq!(err_of(&body)?, UnknownShapeReason::Missing);
    Ok(())
}

#[test]
fn parse_two_block_array_returns_non_three_block() -> anyhow::Result<()> {
    let body = json!({
        "system": [
            {"type": "text", "text": "billing"},
            {"type": "text", "text": "preamble"},
        ]
    });
    assert_eq!(err_of(&body)?, UnknownShapeReason::NonThreeBlock { len: 2 });
    Ok(())
}

#[test]
fn parse_four_block_array_returns_non_three_block() -> anyhow::Result<()> {
    let body = json!({
        "system": [
            {"type": "text", "text": "a"},
            {"type": "text", "text": "b"},
            {"type": "text", "text": "c"},
            {"type": "text", "text": "d"},
        ]
    });
    assert_eq!(err_of(&body)?, UnknownShapeReason::NonThreeBlock { len: 4 });
    Ok(())
}

#[test]
fn parse_empty_array_returns_non_three_block_zero() -> anyhow::Result<()> {
    let body = json!({"system": []});
    assert_eq!(err_of(&body)?, UnknownShapeReason::NonThreeBlock { len: 0 });
    Ok(())
}

#[test]
fn parse_non_text_block_at_index_one_returns_non_text_block() -> anyhow::Result<()> {
    let body = json!({
        "system": [
            {"type": "text", "text": "billing"},
            {"type": "image", "source": {"type": "base64"}},
            {"type": "text", "text": "role body"},
        ]
    });
    assert_eq!(
        err_of(&body)?,
        UnknownShapeReason::NonTextBlock { index: 1 }
    );
    Ok(())
}

#[test]
fn parse_block2_missing_text_field_returns_block2_text_missing() -> anyhow::Result<()> {
    let body = json!({
        "system": [
            {"type": "text", "text": "billing"},
            {"type": "text", "text": "preamble"},
            {"type": "text"},
        ]
    });
    assert_eq!(err_of(&body)?, UnknownShapeReason::Block2TextMissing);
    Ok(())
}

#[test]
fn signature_hash_stable_across_runs() {
    let v = serde_json::json!({"reason": "non_3_block", "len": 2});
    let h1 = signature_hash_for_unknown_blocks(&v);
    let h2 = signature_hash_for_unknown_blocks(&v);
    assert_eq!(
        h1, h2,
        "hashing the same value twice must yield the same hex"
    );
    assert_eq!(h1.len(), 64);
    assert!(h1.chars().all(|c| c.is_ascii_hexdigit()));
}

#[test]
fn signature_hash_independent_of_json_key_order() {
    let a = serde_json::json!({"reason": "non_3_block", "len": 2});
    let b = serde_json::json!({"len": 2, "reason": "non_3_block"});
    assert_eq!(
        signature_hash_for_unknown_blocks(&a),
        signature_hash_for_unknown_blocks(&b),
    );
}

#[test]
fn signature_hash_differs_for_different_reason() {
    let a = serde_json::json!({"reason": "non_3_block", "len": 2});
    let b = serde_json::json!({"reason": "non_3_block", "len": 4});
    assert_ne!(
        signature_hash_for_unknown_blocks(&a),
        signature_hash_for_unknown_blocks(&b),
    );
}

#[test]
fn signature_hash_array_element_order_is_significant() {
    let a = serde_json::json!(["a", "b"]);
    let b = serde_json::json!(["b", "a"]);
    assert_ne!(
        signature_hash_for_unknown_blocks(&a),
        signature_hash_for_unknown_blocks(&b),
    );
}

#[test]
fn signature_hash_handles_full_text_without_truncation() {
    let big = "x".repeat(40_000);
    let value = serde_json::json!({"reason": "non_3_block", "blocks": [{"text": big}]});
    let hash = signature_hash_for_unknown_blocks(&value);
    assert_eq!(hash.len(), 64);
}

#[test]
fn unknown_shape_reason_renders_to_expected_json() {
    assert_eq!(
        UnknownShapeReason::Missing.to_json(),
        json!({"reason": "missing"})
    );
    assert_eq!(
        UnknownShapeReason::StringForm.to_json(),
        json!({"reason": "string_form"})
    );
    assert_eq!(
        UnknownShapeReason::NonThreeBlock { len: 4 }.to_json(),
        json!({"reason": "non_3_block", "len": 4})
    );
    assert_eq!(
        UnknownShapeReason::NonTextBlock { index: 0 }.to_json(),
        json!({"reason": "non_text_block", "index": 0})
    );
    assert_eq!(
        UnknownShapeReason::Block2TextMissing.to_json(),
        json!({"reason": "block_2_text_missing"})
    );
}
