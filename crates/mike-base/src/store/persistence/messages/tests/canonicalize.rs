use serde_json::json;

use crate::store::persistence::messages::{canonicalise_body_messages, hex_lowercase};

#[test]
fn canonicalise_body_messages_is_key_order_independent() -> anyhow::Result<()> {
    let body_a = serde_json::to_vec(&json!({
        "messages": [
            { "role": "user", "content": "hi" }
        ],
        "model": "claude-sonnet-4"
    }))?;
    let body_b = serde_json::to_vec(&json!({
        "model": "claude-sonnet-4",
        "messages": [
            { "content": "hi", "role": "user" }
        ]
    }))?;

    let a = canonicalise_body_messages(&body_a)?;
    let b = canonicalise_body_messages(&body_b)?;
    assert_eq!(
        a, b,
        "permuted key order must produce identical canonical output"
    );
    assert_eq!(a.len(), 1);
    assert_eq!(a[0].role, "user");
    assert_eq!(a[0].byte_size as usize, a[0].body_canonical_bytes.len());
    Ok(())
}

#[test]
fn canonicalise_handles_nested_object_key_permutation() -> anyhow::Result<()> {
    let body_a = serde_json::to_vec(&json!({
        "messages": [
            { "role": "user", "content": [
                { "type": "text", "text": "hello" }
            ]}
        ]
    }))?;
    let body_b = serde_json::to_vec(&json!({
        "messages": [
            { "role": "user", "content": [
                { "text": "hello", "type": "text" }
            ]}
        ]
    }))?;
    let a = canonicalise_body_messages(&body_a)?;
    let b = canonicalise_body_messages(&body_b)?;
    assert_eq!(a, b, "permuted nested object keys must hash identically");
    Ok(())
}

#[test]
fn canonicalise_strips_cache_control_before_hashing() -> anyhow::Result<()> {
    let with_cc = serde_json::to_vec(&json!({
        "messages": [
            { "role": "user", "content": "hi", "cache_control": {"type": "ephemeral"} }
        ]
    }))?;
    let without_cc = serde_json::to_vec(&json!({
        "messages": [
            { "role": "user", "content": "hi" }
        ]
    }))?;
    let a = canonicalise_body_messages(&with_cc)?;
    let b = canonicalise_body_messages(&without_cc)?;
    assert_eq!(
        a[0].sha, b[0].sha,
        "cache_control must be stripped BEFORE hashing; otherwise dedup is 0% on every turn"
    );
    assert_eq!(
        a[0].body_canonical_bytes, b[0].body_canonical_bytes,
        "cache_control must not survive into the canonical (dedup'd) bytes"
    );
    assert_eq!(
        a[0].cache_control,
        Some(json!([{"pointer": "/cache_control", "value": {"type": "ephemeral"}}])),
        "the stripped cache_control must be captured on the CanonicalMessage, not merely discarded (#68 step 2)"
    );
    assert_eq!(
        b[0].cache_control, None,
        "a message with no cache_control on the wire must record none"
    );
    Ok(())
}

#[test]
fn canonicalise_strips_cache_control_recursively() -> anyhow::Result<()> {
    let with_nested_cc = serde_json::to_vec(&json!({
        "messages": [
            { "role": "user", "content": [
                { "type": "text", "text": "hello", "cache_control": {"type": "ephemeral"} }
            ]}
        ]
    }))?;
    let without = serde_json::to_vec(&json!({
        "messages": [
            { "role": "user", "content": [
                { "type": "text", "text": "hello" }
            ]}
        ]
    }))?;
    let a = canonicalise_body_messages(&with_nested_cc)?;
    let b = canonicalise_body_messages(&without)?;
    assert_eq!(
        a[0].sha, b[0].sha,
        "cache_control must be stripped at every nesting level before hashing"
    );
    assert_eq!(
        a[0].cache_control,
        Some(json!([{"pointer": "/content/0/cache_control", "value": {"type": "ephemeral"}}])),
        "the pointer must locate the exact nested position cache_control was removed from"
    );
    Ok(())
}

#[test]
fn canonicalise_returns_empty_for_missing_messages_field() -> anyhow::Result<()> {
    let body = serde_json::to_vec(&json!({"model": "claude-sonnet-4"}))?;
    let canonical = canonicalise_body_messages(&body)?;
    assert!(canonical.is_empty());
    Ok(())
}

#[test]
fn canonicalise_rejects_non_array_messages_field() -> anyhow::Result<()> {
    let body = serde_json::to_vec(&json!({"messages": "not-an-array"}))?;
    let err = canonicalise_body_messages(&body)
        .err()
        .ok_or_else(|| anyhow::anyhow!("expected canonicalise to error, got Ok"))?;
    assert!(err.to_string().contains("not a JSON array"));
    Ok(())
}

#[test]
fn canonicalise_accepts_system_role() -> anyhow::Result<()> {
    let body = serde_json::to_vec(&json!({
        "messages": [{"role": "system", "content": "mid-session policy update"}]
    }))?;
    let out = canonicalise_body_messages(&body)?;
    assert_eq!(out.len(), 1, "one message expected");
    assert_eq!(out[0].role, "system", "system role preserved on the row");
    assert_eq!(out[0].ord, 0);
    assert!(
        !out[0].body_canonical_bytes.is_empty(),
        "canonical bytes must be emitted for a system message"
    );
    Ok(())
}

#[test]
fn canonicalise_rejects_unknown_role() -> anyhow::Result<()> {
    let body = serde_json::to_vec(&json!({
        "messages": [{"role": "tool", "content": "hi"}]
    }))?;
    let err = canonicalise_body_messages(&body)
        .err()
        .ok_or_else(|| anyhow::anyhow!("expected canonicalise to error, got Ok"))?;
    let msg = err.to_string();
    assert!(
        msg.contains("\"tool\""),
        "error must name the bad role: {msg}"
    );
    assert!(
        msg.contains("user, assistant, system"),
        "error must list valid roles: {msg}"
    );
    Ok(())
}

#[test]
fn canonicalise_rejects_missing_role() -> anyhow::Result<()> {
    let body = serde_json::to_vec(&json!({
        "messages": [
            {"role": "user", "content": "ok"},
            {"content": "no role here"}
        ]
    }))?;
    let err = canonicalise_body_messages(&body)
        .err()
        .ok_or_else(|| anyhow::anyhow!("expected canonicalise to error, got Ok"))?;
    let msg = err.to_string();
    assert!(
        msg.contains("messages[1]"),
        "error must name the offending index: {msg}"
    );
    assert!(
        msg.contains("`role`"),
        "error must name the missing field: {msg}"
    );
    Ok(())
}

#[test]
fn canonicalise_preserves_order_and_is_idempotent() -> anyhow::Result<()> {
    let body = serde_json::to_vec(&json!({
        "messages": [
            {"role": "user", "content": "hello"},
            {"role": "assistant", "content": "hi"},
            {"role": "user", "content": "next"}
        ]
    }))?;
    let a = canonicalise_body_messages(&body)?;
    let b = canonicalise_body_messages(&body)?;
    assert_eq!(a, b, "canonicalisation must be deterministic across calls");
    assert_eq!(a.len(), 3);
    assert_eq!(a[0].ord, 0);
    assert_eq!(a[1].ord, 1);
    assert_eq!(a[2].ord, 2);
    assert_eq!(a[0].role, "user");
    assert_eq!(a[1].role, "assistant");
    assert_ne!(a[0].sha, a[2].sha);
    Ok(())
}

#[test]
fn canonical_bytes_have_no_whitespace() -> anyhow::Result<()> {
    let body = serde_json::to_vec(&json!({
        "messages": [
            {"role": "user", "content": [
                {"type": "text", "text": "spaces and\ttabs and\nnewlines"}
            ]}
        ]
    }))?;
    let canonical = canonicalise_body_messages(&body)?;
    let bytes = &canonical[0].body_canonical_bytes;
    assert!(!bytes.contains(&b'\n'));
    assert!(!bytes.contains(&b'\t'));
    Ok(())
}

#[test]
fn hex_lowercase_matches_messages_sha256_check() {
    let bytes = [0xab; 32];
    let hex = hex_lowercase(&bytes);
    assert_eq!(hex.len(), 64);
    assert!(
        hex.chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
    );
    assert_eq!(hex, "ab".repeat(32));
}
