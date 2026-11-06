use super::content_blocks::transform_body;
use super::content_blocks_test_support::{body_bytes, cfg, rewrote, rule};
use super::{FilterAction, FilterKind, FilterOutcome};
use serde_json::json;

#[test]
fn drops_system_text_block_by_xml_tag() -> anyhow::Result<()> {
    let body = body_bytes(json!({
        "model": "m",
        "system": [
            {"type": "text", "text": "<system-reminder>noise</system-reminder>"},
            {"type": "text", "text": "You are Claude Code.", "cache_control": {"type": "ephemeral"}},
        ],
        "messages": [],
    }))?;
    let c = cfg(vec![rule(
        "drop",
        FilterKind::XmlTag,
        "system-reminder",
        FilterAction::Drop,
        None,
    )]);
    let (v, _) = rewrote(transform_body(&body, &c, ""))?;
    let arr = v["system"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("system must be array"))?;
    assert_eq!(arr.len(), 1, "reminder dropped");
    assert_eq!(arr[0]["text"], "You are Claude Code.");
    assert_eq!(arr[0]["cache_control"]["type"], "ephemeral");
    Ok(())
}

#[test]
fn drops_messages_content_blocks_not_just_system() -> anyhow::Result<()> {
    let body = body_bytes(json!({
        "messages": [{
            "role": "user",
            "content": [
                {"type": "text", "text": "<system-reminder>noise</system-reminder>"},
                {"type": "text", "text": "Actual user question."}
            ]
        }]
    }))?;
    let c = cfg(vec![rule(
        "drop",
        FilterKind::XmlTag,
        "system-reminder",
        FilterAction::Drop,
        None,
    )]);
    let (v, _) = rewrote(transform_body(&body, &c, ""))?;
    let arr = v["messages"][0]["content"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("content must be array"))?;
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["text"], "Actual user question.");
    Ok(())
}

#[test]
fn system_walk_skips_slice_replace_inside_a_global_scoped_block() -> anyhow::Result<()> {
    let body = body_bytes(json!({
        "system": [
            {"type": "text", "text": "billing-header"},
            {"type": "text", "text": "identity preamble"},
            {
                "type": "text",
                "text": "secret harness details and more",
                "cache_control": {"type": "ephemeral", "scope": "global", "ttl": "1h"},
            },
        ],
    }))?;
    let c = cfg(vec![rule(
        "replace",
        FilterKind::TextSubstring,
        "secret harness details",
        FilterAction::Replace,
        Some("MUTATED"),
    )]);
    match transform_body(&body, &c, "") {
        FilterOutcome::Unchanged => Ok(()),
        FilterOutcome::Rewrote { .. } => Err(anyhow::anyhow!(
            "expected Unchanged — a globally-scoped block must never be mutated"
        )),
    }
}

#[test]
fn system_walk_skips_append_anchored_before_the_global_block() -> anyhow::Result<()> {
    let body = body_bytes(json!({
        "system": [
            {"type": "text", "text": "billing-header"},
            {"type": "text", "text": "identity preamble"},
            {
                "type": "text",
                "text": "role body",
                "cache_control": {"type": "ephemeral", "scope": "global", "ttl": "1h"},
            },
        ],
    }))?;
    let c = cfg(vec![rule(
        "append",
        FilterKind::TextSubstring,
        "identity preamble",
        FilterAction::Append,
        Some("INJECTED"),
    )]);
    match transform_body(&body, &c, "") {
        FilterOutcome::Unchanged => Ok(()),
        FilterOutcome::Rewrote { .. } => Err(anyhow::anyhow!(
            "expected Unchanged — an Append anchored before the global block must not insert ahead of it"
        )),
    }
}

#[test]
fn whole_block_replace_preserves_type_and_cache_control() -> anyhow::Result<()> {
    let body = body_bytes(json!({
        "system": [{
            "type": "text",
            "text": "The following skills are available in this session.",
            "cache_control": {"type": "ephemeral"},
        }]
    }))?;
    let c = cfg(vec![rule(
        "replace",
        FilterKind::TextSubstring,
        "the following skills are available in this session.",
        FilterAction::Replace,
        Some("Skills present; invoke by name."),
    )]);
    let (v, _) = rewrote(transform_body(&body, &c, ""))?;
    let block = &v["system"][0];
    assert_eq!(block["type"], "text");
    assert_eq!(block["text"], "Skills present; invoke by name.");
    assert_eq!(block["cache_control"]["type"], "ephemeral");
    Ok(())
}

#[test]
fn whole_block_append_inserts_after_match_without_mutating_original() -> anyhow::Result<()> {
    let body = body_bytes(json!({
        "system": [
            {"type": "text", "text": "You are an interactive agent.", "cache_control": {"type": "ephemeral"}},
            {"type": "text", "text": "Other block."}
        ]
    }))?;
    let c = cfg(vec![rule(
        "append",
        FilterKind::TextSubstring,
        "you are an interactive agent.",
        FilterAction::Append,
        Some("House rules: prefer tool calls."),
    )]);
    let (v, _) = rewrote(transform_body(&body, &c, ""))?;
    let arr = v["system"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("system must be array"))?;
    assert_eq!(arr.len(), 3, "one appended block");
    assert_eq!(arr[0]["text"], "You are an interactive agent.");
    assert_eq!(arr[0]["cache_control"]["type"], "ephemeral");
    assert_eq!(arr[1]["type"], "text");
    assert_eq!(arr[1]["text"], "House rules: prefer tool calls.");
    assert!(
        arr[1].get("cache_control").is_none(),
        "appended block must not carry cache_control"
    );
    assert_eq!(arr[2]["text"], "Other block.");
    Ok(())
}
